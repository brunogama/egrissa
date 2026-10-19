use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{
        CredentialRef, EndpointId, NetworkIdentityId, OperatorId, RequestId, RouteId,
        UpstreamBindingId, WorkloadId,
    },
    policy::{EgressPolicy, VerificationPolicy},
    request::{
        Destination, DestinationKey, EgressRequest, GeographyRequirement, IdentityRequirement,
        IsolationRequirement, PrivacyRequirement, TransportRequirement,
    },
};
use egress_runtime::{
    AcquireContext, Clock, DataPlaneTransportAdapter, EgressRuntime, TunnelTransportAdapter,
};
use http_connect_adapter::{
    HttpConnectAdapter, HttpConnectAddressResolver, HttpConnectCredentialResolver,
    HttpConnectCredentials,
};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use masque_adapter::{
    MasqueAdapter, MasqueAddressResolver, MasqueAttemptFailure, MasqueConnector,
    MasqueCredentialResolver, MasqueCredentials, MasqueOpenFuture, MasqueProxy, MasqueService,
    MasqueTarget,
};
use observation_core::EgressObservation;
use relay_adapter::{
    RelayAddressResolver, RelayClient, RelayCredentialResolver, RelayCredentials, RelayTarget,
};
use route_core::{
    planner::{Candidate, DestinationEligibility, Endpoint, InventorySnapshot, UpstreamBinding},
    topology::RouteTopology,
};
use socks5_adapter::{
    Socks5Adapter, Socks5AddressResolver, Socks5CredentialResolver, Socks5Credentials,
};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use transport_core::Transport;
use verification_core::EvidenceVerifier;

#[derive(Clone, Copy)]
struct TestClock(Instant);

impl Clock for TestClock {
    fn now(&self) -> Instant {
        self.0
    }
}

struct NoopSocksAddresses;

impl Socks5AddressResolver for NoopSocksAddresses {
    fn gateway_address(&self, _gateway: EndpointId) -> Option<SocketAddr> {
        None
    }

    fn destination_address(&self, _destination: DestinationKey, _port: u16) -> Option<SocketAddr> {
        None
    }
}

struct NoopSocksCredentials;

impl Socks5CredentialResolver for NoopSocksCredentials {
    fn resolve(&self, _reference: CredentialRef) -> Option<Socks5Credentials> {
        None
    }
}

struct NoopHttpAddresses;

impl HttpConnectAddressResolver for NoopHttpAddresses {
    fn proxy_address(&self, _proxy: EndpointId) -> Option<SocketAddr> {
        None
    }

    fn destination(&self, _key: DestinationKey, _port: u16) -> Option<(String, u16)> {
        None
    }
}

struct NoopHttpCredentials;

impl HttpConnectCredentialResolver for NoopHttpCredentials {
    fn resolve(&self, _reference: CredentialRef) -> Option<HttpConnectCredentials> {
        None
    }
}

struct NoopRelayAddresses;

impl RelayAddressResolver for NoopRelayAddresses {
    fn endpoint_address(&self, _endpoint: EndpointId) -> Option<SocketAddr> {
        None
    }

    fn destination_address(&self, _destination: DestinationKey, _port: u16) -> Option<RelayTarget> {
        None
    }
}

struct NoopRelayCredentials;

impl RelayCredentialResolver for NoopRelayCredentials {
    fn resolve(&self, _reference: CredentialRef) -> Option<RelayCredentials> {
        None
    }
}

struct NoopMasqueAddresses;

impl MasqueAddressResolver for NoopMasqueAddresses {
    fn proxy(&self, _endpoint: EndpointId) -> Option<MasqueProxy> {
        None
    }

    fn target(
        &self,
        _destination: DestinationKey,
        _port: u16,
        _service: MasqueService,
    ) -> Option<MasqueTarget> {
        None
    }
}

struct NoopMasqueCredentials;

impl MasqueCredentialResolver for NoopMasqueCredentials {
    fn resolve(&self, _reference: CredentialRef) -> Option<MasqueCredentials> {
        None
    }
}

struct NoopMasqueConnector;

impl MasqueConnector for NoopMasqueConnector {
    fn open(
        &self,
        _proxy: SocketAddr,
        _request: masque_adapter::MasqueConnectRequest,
    ) -> MasqueOpenFuture {
        Box::pin(async { Err(MasqueAttemptFailure::BeforeRequest) })
    }
}

#[test]
fn concrete_adapters_wrap_as_transport() {
    fn assert_transport<T: Transport>() {}

    assert_transport::<
        TunnelTransportAdapter<Socks5Adapter<NoopSocksAddresses, NoopSocksCredentials>>,
    >();
    assert_transport::<
        TunnelTransportAdapter<HttpConnectAdapter<NoopHttpAddresses, NoopHttpCredentials>>,
    >();
    assert_transport::<TunnelTransportAdapter<RelayClient<NoopRelayAddresses, NoopRelayCredentials>>>(
    );
    assert_transport::<
        DataPlaneTransportAdapter<
            MasqueAdapter<NoopMasqueAddresses, NoopMasqueCredentials, NoopMasqueConnector>,
        >,
    >();
}

struct FixtureAddresses {
    gateway: SocketAddr,
    destination: SocketAddr,
}

impl Socks5AddressResolver for FixtureAddresses {
    fn gateway_address(&self, _gateway: EndpointId) -> Option<SocketAddr> {
        Some(self.gateway)
    }

    fn destination_address(&self, _destination: DestinationKey, _port: u16) -> Option<SocketAddr> {
        Some(self.destination)
    }
}

struct FixtureCredentials;

impl Socks5CredentialResolver for FixtureCredentials {
    fn resolve(&self, _reference: CredentialRef) -> Option<Socks5Credentials> {
        None
    }
}

fn context(now: Instant) -> AcquireContext {
    let mut identity = NetworkIdentity::allocate(
        NetworkIdentityId(1),
        WorkloadId(1),
        IdentityScope::Workload,
        now,
        None,
    )
    .unwrap();
    identity
        .transition(identity.id(), 0, IdentityState::Active, now)
        .unwrap();
    let capabilities = EgressCapabilities {
        transport: TransportCapabilities {
            tcp: true,
            ..Default::default()
        },
        identity: IdentityCapabilities {
            sticky: true,
            continuity_observable: true,
            ..Default::default()
        },
        topology: TopologyCapabilities {
            supported: vec![TopologyKind::Socks5],
            ..Default::default()
        },
        isolation: IsolationCapabilities {
            shared: true,
            ..Default::default()
        },
        ..Default::default()
    };
    AcquireContext {
        request: EgressRequest {
            id: RequestId(1),
            workload: WorkloadId(1),
            destination: Destination {
                key: DestinationKey(2),
                port: 443,
            },
            identity: IdentityRequirement::Ephemeral,
            transport: TransportRequirement::Tcp,
            geography: GeographyRequirement::Any,
            isolation: IsolationRequirement::Shared,
            privacy: PrivacyRequirement::Standard,
        },
        policy: EgressPolicy {
            verification: VerificationPolicy::RequireBeforeUse,
            ..Default::default()
        },
        identity,
        inventory: InventorySnapshot {
            endpoints: vec![Endpoint {
                id: EndpointId(1),
                operator: OperatorId(1),
                enabled: true,
                available: true,
                destinations: DestinationEligibility::Any,
                capabilities: capabilities.clone(),
                valid_until: Some(now + Duration::from_secs(60)),
                credential: None,
            }],
            upstreams: vec![UpstreamBinding {
                id: UpstreamBindingId(1),
                operator: OperatorId(2),
                provider: egress_core::ids::ProviderId(1),
                available: true,
                destinations: DestinationEligibility::Any,
                capabilities,
                valid_until: Some(now + Duration::from_secs(60)),
                credential: None,
            }],
            candidates: vec![Candidate {
                topology: RouteTopology::Socks5 {
                    gateway: EndpointId(1),
                    upstream: UpstreamBindingId(1),
                },
                rank: 0,
            }],
            credentials: vec![],
        },
        observations: Default::default(),
        route: RouteId(1),
        route_generation: 0,
        now,
        egress_observations: vec![EgressObservation {
            exit_address: Some("203.0.113.7".parse().unwrap()),
            ..Default::default()
        }],
    }
}

async fn read_exact(stream: &mut tokio::net::TcpStream, length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await.unwrap();
    bytes
}

#[tokio::test]
async fn socks5_adapter_wrapper_establishes_verifies_and_closes_the_retained_tunnel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_exact(&mut stream, 3).await, [5, 1, 0]);
        stream.write_all(&[5, 0]).await.unwrap();
        assert_eq!(read_exact(&mut stream, 4).await, [5, 1, 0, 1]);
        assert_eq!(read_exact(&mut stream, 4).await, [127, 0, 0, 1]);
        assert_eq!(
            read_exact(&mut stream, 2).await,
            address.port().to_be_bytes()
        );
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
            .await
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 0);
    });
    let adapter = Socks5Adapter::new(
        FixtureAddresses {
            gateway: address,
            destination: address,
        },
        FixtureCredentials,
    );
    let wrapper = TunnelTransportAdapter::new(adapter);
    let now = Instant::now();
    let runtime =
        EgressRuntime::with_verifier(wrapper.clone(), TestClock(now), Arc::new(EvidenceVerifier));

    let lease = runtime.acquire(context(now)).await.unwrap();
    assert!(lease.verification.is_some());
    let key = lease.lease.plan.key();
    Transport::close(&wrapper, key).await;
    Transport::close(&wrapper, key).await;
    server.await.unwrap();
}
