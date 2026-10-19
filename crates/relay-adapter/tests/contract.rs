use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{
        EndpointId, NetworkIdentityId, OperatorId, RequestId, RouteId, UpstreamBindingId,
        WorkloadId,
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
use relay_adapter::{
    RelayAddressResolver, RelayClient, RelayCredentialResolver, RelayCredentials,
    RelayForwardingPolicy, RelayServer, RelayTarget,
};
use route_core::{
    planner::{
        Candidate, DestinationEligibility, DeterministicPlanner, Endpoint, FirstCandidate,
        InventorySnapshot, PlanContext, RoutePlanner, UpstreamBinding,
    },
    topology::RouteTopology,
};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use transport_core::{EstablishmentRequest, TunnelTransport};

#[derive(Default)]
struct Addresses {
    endpoints: BTreeMap<EndpointId, SocketAddr>,
    destination: Option<RelayTarget>,
}

impl RelayAddressResolver for Addresses {
    fn endpoint_address(&self, endpoint: EndpointId) -> Option<SocketAddr> {
        self.endpoints.get(&endpoint).copied()
    }

    fn destination_address(&self, _: DestinationKey, _: u16) -> Option<RelayTarget> {
        self.destination
    }
}

#[derive(Default)]
struct Credentials;

impl RelayCredentialResolver for Credentials {
    fn resolve(&self, _: egress_core::ids::CredentialRef) -> Option<RelayCredentials> {
        None
    }
}

fn request(
    topology: RouteTopology,
    destination: SocketAddr,
    credential: Option<egress_core::ids::CredentialRef>,
) -> EstablishmentRequest {
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
            continuity_observable: true,
            ..Default::default()
        },
        topology: TopologyCapabilities {
            supported: vec![
                TopologyKind::Relay,
                TopologyKind::RelayThenSocks5,
                TopologyKind::TwoRelaysThenSocks5,
                TopologyKind::Socks5,
            ],
            separate_client_and_destination: true,
        },
        isolation: IsolationCapabilities {
            shared: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let endpoints = topology
        .endpoints()
        .into_iter()
        .enumerate()
        .map(|(index, (id, _))| Endpoint {
            id,
            operator: OperatorId((index + 1) as u128),
            enabled: true,
            available: true,
            destinations: DestinationEligibility::Any,
            capabilities: capabilities.clone(),
            valid_until: Some(now + Duration::from_secs(60)),
            credential: (index == 0).then_some(credential).flatten(),
        })
        .collect();
    let destination = Destination {
        key: DestinationKey(1),
        port: destination.port(),
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
    let inventory = InventorySnapshot {
        endpoints,
        upstreams: topology
            .upstream()
            .into_iter()
            .map(|id| UpstreamBinding {
                id,
                operator: OperatorId(4),
                provider: egress_core::ids::ProviderId(1),
                available: true,
                destinations: DestinationEligibility::Any,
                capabilities: capabilities.clone(),
                valid_until: Some(now + Duration::from_secs(60)),
                credential: None,
            })
            .collect(),
        candidates: vec![Candidate { topology, rank: 0 }],
        credentials: credential
            .into_iter()
            .map(|reference| route_core::planner::CredentialLease {
                reference,
                valid_until: None,
            })
            .collect(),
    };
    let plan = DeterministicPlanner::default()
        .plan(
            &PlanContext {
                request: &egress,
                policy: &EgressPolicy::default(),
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

async fn relay(listener: TcpListener, allowed: RelayTarget) {
    let (stream, _) = listener.accept().await.unwrap();
    RelayServer::new(RelayForwardingPolicy::allow_only([allowed]))
        .serve(stream)
        .await
        .unwrap();
}

async fn socks5(listener: TcpListener) {
    let (mut inbound, _) = listener.accept().await.unwrap();
    let mut greeting = [0; 3];
    inbound.read_exact(&mut greeting).await.unwrap();
    assert_eq!(greeting, [5, 1, 0]);
    inbound.write_all(&[5, 0]).await.unwrap();
    let mut request = [0; 10];
    inbound.read_exact(&mut request).await.unwrap();
    assert_eq!(&request[..4], &[5, 1, 0, 1]);
    let target = SocketAddr::from((
        [request[4], request[5], request[6], request[7]],
        u16::from_be_bytes([request[8], request[9]]),
    ));
    let mut outbound = TcpStream::connect(target).await.unwrap();
    inbound
        .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
        .await
        .unwrap();
    tokio::io::copy_bidirectional(&mut inbound, &mut outbound)
        .await
        .unwrap();
}

async fn echo(listener: TcpListener) {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut message = [0; 4];
    stream.read_exact(&mut message).await.unwrap();
    assert_eq!(&message, b"ping");
    stream.write_all(b"pong").await.unwrap();
}

#[test]
fn forwarding_policy_allows_only_explicit_targets() {
    let allowed: SocketAddr = "127.0.0.1:1080".parse().unwrap();
    let blocked: SocketAddr = "127.0.0.1:1081".parse().unwrap();
    let policy = RelayForwardingPolicy::allow_only([RelayTarget::new(allowed)]);

    assert!(policy.allows(RelayTarget::new(allowed)));
    assert!(!policy.allows(RelayTarget::new(blocked)));
}

#[tokio::test]
async fn relay_server_rejects_targets_outside_the_allowlist() {
    let allowed: SocketAddr = "127.0.0.1:1080".parse().unwrap();
    let blocked: SocketAddr = "127.0.0.1:1081".parse().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(relay(listener, RelayTarget::new(allowed)));

    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("CONNECT {blocked} HTTP/1.1\r\nHost: {blocked}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = vec![0; 64];
    let read = stream.read(&mut response).await.unwrap();
    assert!(
        std::str::from_utf8(&response[..read])
            .unwrap()
            .starts_with("HTTP/1.1 403")
    );
    server.await.unwrap();
}

#[tokio::test]
async fn relay_client_builds_a_one_relay_hop_to_socks5() {
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_address = echo_listener.local_addr().unwrap();
    let socks_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socks_address = socks_listener.local_addr().unwrap();
    let relay_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_address = relay_listener.local_addr().unwrap();
    let echo_task = tokio::spawn(echo(echo_listener));
    let socks_task = tokio::spawn(socks5(socks_listener));
    let relay_task = tokio::spawn(relay(relay_listener, RelayTarget::new(socks_address)));
    let client = RelayClient::new(
        Addresses {
            endpoints: BTreeMap::from([
                (EndpointId(1), relay_address),
                (EndpointId(2), socks_address),
            ]),
            destination: Some(RelayTarget::new(echo_address)),
        },
        Credentials,
    );
    let mut tunnel = client
        .establish_tunnel(request(
            RouteTopology::RelayThenSocks5 {
                relay: EndpointId(1),
                gateway: EndpointId(2),
                upstream: UpstreamBindingId(1),
            },
            echo_address,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(tunnel.write(b"ping".to_vec()).await.unwrap(), 4);
    assert_eq!(tunnel.read(4).await.unwrap(), b"pong");
    tunnel.close().await.unwrap();
    echo_task.await.unwrap();
    socks_task.await.unwrap();
    relay_task.await.unwrap();
}

#[tokio::test]
async fn relay_client_builds_two_relay_hops_to_socks5() {
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_address = echo_listener.local_addr().unwrap();
    let socks_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socks_address = socks_listener.local_addr().unwrap();
    let egress_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let egress_address = egress_listener.local_addr().unwrap();
    let ingress_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ingress_address = ingress_listener.local_addr().unwrap();
    let echo_task = tokio::spawn(echo(echo_listener));
    let socks_task = tokio::spawn(socks5(socks_listener));
    let egress_task = tokio::spawn(relay(egress_listener, RelayTarget::new(socks_address)));
    let ingress_task = tokio::spawn(relay(ingress_listener, RelayTarget::new(egress_address)));
    let client = RelayClient::new(
        Addresses {
            endpoints: BTreeMap::from([
                (EndpointId(1), ingress_address),
                (EndpointId(2), egress_address),
                (EndpointId(3), socks_address),
            ]),
            destination: Some(RelayTarget::new(echo_address)),
        },
        Credentials,
    );
    let mut tunnel = client
        .establish_tunnel(request(
            RouteTopology::TwoRelaysThenSocks5 {
                ingress: EndpointId(1),
                egress: EndpointId(2),
                gateway: EndpointId(3),
                upstream: UpstreamBindingId(1),
            },
            echo_address,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(tunnel.write(b"ping".to_vec()).await.unwrap(), 4);
    assert_eq!(tunnel.read(4).await.unwrap(), b"pong");
    tunnel.close().await.unwrap();
    echo_task.await.unwrap();
    socks_task.await.unwrap();
    egress_task.await.unwrap();
    ingress_task.await.unwrap();
}

#[tokio::test]
async fn missing_relay_credential_fails_closed_before_connecting() {
    let client = RelayClient::new(
        Addresses {
            endpoints: BTreeMap::from([(EndpointId(1), "127.0.0.1:1".parse().unwrap())]),
            destination: Some(RelayTarget::new("127.0.0.1:2".parse().unwrap())),
        },
        Credentials,
    );

    assert!(matches!(
        client
            .establish_tunnel(request(
                RouteTopology::Relay {
                    relay: EndpointId(1),
                },
                "127.0.0.1:2".parse().unwrap(),
                Some(egress_core::ids::CredentialRef(7)),
            ))
            .await,
        Err(transport_core::TransportFailure::AuthenticationRejected)
    ));
}

#[tokio::test]
async fn relay_server_accepts_only_connect_requests() {
    let target: SocketAddr = "127.0.0.1:1080".parse().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(relay(listener, RelayTarget::new(target)));

    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = vec![0; 64];
    let read = stream.read(&mut response).await.unwrap();
    assert!(
        std::str::from_utf8(&response[..read])
            .unwrap()
            .starts_with("HTTP/1.1 400")
    );
    server.await.unwrap();
}
