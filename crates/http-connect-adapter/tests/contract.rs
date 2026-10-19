use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{
        CredentialRef, EndpointId, NetworkIdentityId, OperatorId, RequestId, RouteId, WorkloadId,
    },
    policy::EgressPolicy,
    request::{
        Destination, DestinationKey, EgressRequest, GeographyRequirement, IdentityRequirement,
        IsolationRequirement, PrivacyRequirement, TransportRequirement,
    },
};
use http_connect_adapter::{
    HttpConnectAdapter, HttpConnectAddressResolver, HttpConnectCredentialResolver,
    HttpConnectCredentials,
};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use observation_core::ObservationSnapshot;
use route_core::{
    planner::{
        Candidate, CredentialLease, DestinationEligibility, DeterministicPlanner, Endpoint,
        FirstCandidate, InventorySnapshot, PlanContext, RoutePlanner,
    },
    topology::RouteTopology,
};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use transport_core::{EstablishmentRequest, TunnelTransport};

struct Addresses(SocketAddr);
impl HttpConnectAddressResolver for Addresses {
    fn proxy_address(&self, _: EndpointId) -> Option<SocketAddr> {
        Some(self.0)
    }
    fn destination(&self, _: DestinationKey, port: u16) -> Option<(String, u16)> {
        Some(("example.test".into(), port))
    }
}
struct Credentials(Option<HttpConnectCredentials>);
impl HttpConnectCredentialResolver for Credentials {
    fn resolve(&self, _: CredentialRef) -> Option<HttpConnectCredentials> {
        self.0.clone()
    }
}

fn request(port: u16, credential: Option<CredentialRef>) -> EstablishmentRequest {
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
            http_connect: true,
            ..Default::default()
        },
        identity: IdentityCapabilities {
            continuity_observable: true,
            ..Default::default()
        },
        topology: TopologyCapabilities {
            supported: vec![TopologyKind::HttpConnect],
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
        port,
    };
    let inventory = InventorySnapshot {
        endpoints: vec![Endpoint {
            id: EndpointId(1),
            operator: OperatorId(1),
            enabled: true,
            available: true,
            destinations: DestinationEligibility::Any,
            capabilities,
            valid_until: Some(now + Duration::from_secs(60)),
            credential,
        }],
        upstreams: vec![],
        candidates: vec![Candidate {
            topology: RouteTopology::HttpConnect {
                proxy: EndpointId(1),
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
    let observations = ObservationSnapshot::default();
    let plan = DeterministicPlanner::default()
        .plan(
            &PlanContext {
                request: &egress,
                policy: &policy,
                identity: &identity,
                inventory: &inventory,
                observations: &observations,
                now,
                route: RouteId(1),
                route_generation: 0,
            },
            &mut FirstCandidate,
        )
        .unwrap();
    EstablishmentRequest { plan, destination }
}

async fn headers(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await.unwrap();
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn local_http_connect_proxy_yields_a_usable_authenticated_tunnel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = headers(&mut stream).await;
        assert!(request.starts_with("CONNECT example.test:"));
        assert!(request.contains("Proxy-Authorization: Basic YWxpY2U6c2VjcmV0"));
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"ping");
        stream.write_all(b"pong").await.unwrap();
    });
    let adapter = HttpConnectAdapter::new(
        Addresses(address),
        Credentials(Some(HttpConnectCredentials::new("alice", "secret"))),
    );
    let mut tunnel = adapter
        .establish_tunnel(request(address.port(), Some(CredentialRef(1))))
        .await
        .unwrap();
    assert_eq!(tunnel.write(b"ping".to_vec()).await.unwrap(), 4);
    assert_eq!(tunnel.read(4).await.unwrap(), b"pong");
    tunnel.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn missing_credential_fails_closed_before_proxy_connection() {
    let adapter =
        HttpConnectAdapter::new(Addresses("127.0.0.1:1".parse().unwrap()), Credentials(None));
    assert!(matches!(
        adapter
            .establish_tunnel(request(443, Some(CredentialRef(1))))
            .await,
        Err(transport_core::TransportFailure::AuthenticationRejected)
    ));
}
