use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{
        CredentialRef, EndpointId, NetworkIdentityId, OperatorId, RequestId, RouteId,
        UpstreamBindingId, WorkloadId,
    },
    policy::EgressPolicy,
    request::{
        Destination, DestinationKey, EgressRequest, GeographyRequirement, IdentityRequirement,
        IsolationRequirement, PrivacyRequirement, TransportRequirement,
    },
};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use observation_core::ObservationSnapshot;
use route_core::{
    planner::{
        Candidate, CredentialLease, DestinationEligibility, DeterministicPlanner, Endpoint,
        FirstCandidate, InventorySnapshot, PlanContext, RoutePlanner, UpstreamBinding,
    },
    topology::RouteTopology,
};
use socks5_adapter::{
    Socks5Adapter, Socks5AddressResolver, Socks5CredentialResolver, Socks5Credentials,
};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use transport_core::{EstablishmentRequest, TunnelTransport};

#[derive(Default)]
struct Addresses {
    gateways: BTreeMap<EndpointId, SocketAddr>,
    destinations: BTreeMap<DestinationKey, SocketAddr>,
}

impl Socks5AddressResolver for Addresses {
    fn gateway_address(&self, gateway: EndpointId) -> Option<SocketAddr> {
        self.gateways.get(&gateway).copied()
    }

    fn destination_address(&self, destination: DestinationKey, _port: u16) -> Option<SocketAddr> {
        self.destinations.get(&destination).copied()
    }
}

#[derive(Default)]
struct Credentials(BTreeMap<CredentialRef, Socks5Credentials>);

impl Socks5CredentialResolver for Credentials {
    fn resolve(&self, reference: CredentialRef) -> Option<Socks5Credentials> {
        self.0.get(&reference).cloned()
    }
}

fn request(listener: SocketAddr, credential: Option<CredentialRef>) -> EstablishmentRequest {
    let now = Instant::now();
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
    let destination = Destination {
        key: DestinationKey(2),
        port: listener.port(),
    };
    let inventory = InventorySnapshot {
        endpoints: vec![Endpoint {
            id: EndpointId(1),
            operator: OperatorId(1),
            enabled: true,
            available: true,
            destinations: DestinationEligibility::Any,
            capabilities: capabilities.clone(),
            valid_until: Some(now + Duration::from_secs(60)),
            credential,
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
        credentials: credential
            .into_iter()
            .map(|reference| CredentialLease {
                reference,
                valid_until: None,
            })
            .collect(),
    };
    let egress = EgressRequest {
        id: RequestId(1),
        workload: WorkloadId(1),
        destination: destination.clone(),
        identity: IdentityRequirement::Ephemeral,
        transport: TransportRequirement::Tcp,
        geography: GeographyRequirement::Any,
        isolation: IsolationRequirement::Shared,
        privacy: PrivacyRequirement::Standard,
    };
    let policy = EgressPolicy::default();
    let plan = DeterministicPlanner::default()
        .plan(
            &PlanContext {
                request: &egress,
                policy: &policy,
                identity: &identity,
                inventory: &inventory,
                observations: &ObservationSnapshot::default(),
                now,
                route: RouteId(1),
                route_generation: 0,
            },
            &mut FirstCandidate,
        )
        .unwrap();
    EstablishmentRequest { plan, destination }
}

async fn read_exact(stream: &mut tokio::net::TcpStream, length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await.unwrap();
    bytes
}

#[tokio::test]
async fn connect_with_username_password_yields_a_usable_tunnel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_exact(&mut stream, 4).await, [5, 2, 0, 2]);
        stream.write_all(&[5, 2]).await.unwrap();
        assert_eq!(read_exact(&mut stream, 2).await, [1, 5]);
        assert_eq!(read_exact(&mut stream, 5).await, b"alice");
        assert_eq!(read_exact(&mut stream, 1).await, [6]);
        assert_eq!(read_exact(&mut stream, 6).await, b"secret");
        stream.write_all(&[1, 0]).await.unwrap();
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
        assert_eq!(read_exact(&mut stream, 4).await, b"ping");
        stream.write_all(b"pong").await.unwrap();
    });
    let credential = CredentialRef(7);
    let adapter = Socks5Adapter::new(
        Addresses {
            gateways: BTreeMap::from([(EndpointId(1), address)]),
            destinations: BTreeMap::from([(DestinationKey(2), address)]),
        },
        Credentials(BTreeMap::from([(
            credential,
            Socks5Credentials::new("alice", "secret"),
        )])),
    );
    let mut tunnel = adapter
        .establish_tunnel(request(address, Some(credential)))
        .await
        .unwrap();
    assert_eq!(tunnel.write(b"ping".to_vec()).await.unwrap(), 4);
    assert_eq!(tunnel.read(4).await.unwrap(), b"pong");
    tunnel.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn missing_credential_reference_is_rejected_before_connection() {
    let address = "127.0.0.1:1".parse().unwrap();
    let adapter = Socks5Adapter::new(
        Addresses {
            gateways: BTreeMap::from([(EndpointId(1), address)]),
            destinations: BTreeMap::from([(DestinationKey(2), address)]),
        },
        Credentials::default(),
    );
    assert!(matches!(
        adapter
            .establish_tunnel(request(address, Some(CredentialRef(7))))
            .await,
        Err(transport_core::TransportFailure::AuthenticationRejected)
    ));
}
