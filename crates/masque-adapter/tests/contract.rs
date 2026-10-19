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
        IsolationRequirement, PrivacyRequirement, Transport, TransportRequirement,
    },
};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use masque_adapter::{
    MasqueAdapter, MasqueAddressResolver, MasqueAdvertisement, MasqueAttemptFailure,
    MasqueConfigurationError, MasqueConnectRequest, MasqueConnector, MasqueCredentialResolver,
    MasqueCredentials, MasqueHttpVersion, MasqueMethod, MasqueOpenFuture, MasqueProxy,
    MasqueService, MasqueTarget,
};
use observation_core::ObservationSnapshot;
use route_core::{
    planner::{
        Candidate, DestinationEligibility, DeterministicPlanner, Endpoint, FirstCandidate,
        InventorySnapshot, PlanContext, RoutePlanner,
    },
    topology::RouteTopology,
};
use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use transport_core::{
    DataPlaneTransport, DataPlaneTunnel, DatagramTunnel, EstablishmentRequest, PacketTunnel,
    TunnelCloseFuture, TunnelReadFuture, TunnelWriteFuture,
};

#[derive(Default)]
struct Addresses;

impl MasqueAddressResolver for Addresses {
    fn proxy(&self, _: EndpointId) -> Option<MasqueProxy> {
        Some(MasqueProxy {
            address: "127.0.0.1:443".parse().unwrap(),
            authority: "proxy.example".into(),
        })
    }

    fn target(&self, _: DestinationKey, _: u16, service: MasqueService) -> Option<MasqueTarget> {
        let path = match service {
            MasqueService::ConnectUdp => "/.well-known/masque/udp/192.0.2.6/443/",
            MasqueService::ConnectIp => "/.well-known/masque/ip/*/*",
        };
        MasqueTarget::new("https", path)
    }
}

struct Credentials(Option<MasqueCredentials>);

impl MasqueCredentialResolver for Credentials {
    fn resolve(&self, _: CredentialRef) -> Option<MasqueCredentials> {
        self.0.clone()
    }
}

struct DatagramEcho {
    received: VecDeque<Vec<u8>>,
}

impl DatagramTunnel for DatagramEcho {
    fn send(&mut self, datagram: Vec<u8>) -> TunnelWriteFuture<'_> {
        let length = datagram.len();
        self.received.push_back(datagram);
        Box::pin(async move { Ok(length) })
    }

    fn receive(&mut self, _: usize) -> TunnelReadFuture<'_> {
        let datagram = self.received.pop_front().unwrap();
        Box::pin(async move { Ok(datagram) })
    }

    fn close(self: Box<Self>) -> TunnelCloseFuture {
        Box::pin(async { Ok(()) })
    }
}

struct PacketEcho {
    received: VecDeque<Vec<u8>>,
}

impl PacketTunnel for PacketEcho {
    fn send(&mut self, packet: Vec<u8>) -> TunnelWriteFuture<'_> {
        let length = packet.len();
        self.received.push_back(packet);
        Box::pin(async move { Ok(length) })
    }

    fn receive(&mut self, _: usize) -> TunnelReadFuture<'_> {
        let packet = self.received.pop_front().unwrap();
        Box::pin(async move { Ok(packet) })
    }

    fn close(self: Box<Self>) -> TunnelCloseFuture {
        Box::pin(async { Ok(()) })
    }
}

struct LocalMasque {
    h3_failure: Option<MasqueAttemptFailure>,
    attempts: Arc<Mutex<Vec<MasqueConnectRequest>>>,
}

impl LocalMasque {
    fn available() -> Self {
        Self {
            h3_failure: None,
            attempts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_h3_failure(failure: MasqueAttemptFailure) -> Self {
        Self {
            h3_failure: Some(failure),
            attempts: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl MasqueConnector for LocalMasque {
    fn open(&self, _: SocketAddr, request: MasqueConnectRequest) -> MasqueOpenFuture {
        self.attempts.lock().unwrap().push(request.clone());
        if request.version == MasqueHttpVersion::Http3 {
            if let Some(failure) = self.h3_failure {
                return Box::pin(async move { Err(failure) });
            }
        }
        let tunnel = match request.service {
            MasqueService::ConnectUdp => DataPlaneTunnel::Datagram(Box::new(DatagramEcho {
                received: VecDeque::new(),
            })),
            MasqueService::ConnectIp => DataPlaneTunnel::Packet(Box::new(PacketEcho {
                received: VecDeque::new(),
            })),
        };
        Box::pin(async move { Ok(tunnel) })
    }
}

fn request(transport: Transport, credential: Option<CredentialRef>) -> EstablishmentRequest {
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
            udp: true,
            tcp: true,
            http_connect: true,
            ip: true,
            connect_udp: true,
            connect_ip: true,
            http2: true,
            http3: true,
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
        port: 443,
    };
    let egress = EgressRequest {
        id: RequestId(1),
        workload: WorkloadId(1),
        destination: destination.clone(),
        identity: IdentityRequirement::Ephemeral,
        transport: match transport {
            Transport::Udp => TransportRequirement::Udp,
            Transport::Ip => TransportRequirement::Ip,
            Transport::Tcp => TransportRequirement::Tcp,
        },
        geography: GeographyRequirement::Any,
        isolation: IsolationRequirement::Shared,
        privacy: PrivacyRequirement::Standard,
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

#[tokio::test]
async fn local_connector_receives_rfc9298_connect_udp_and_preserves_datagrams() {
    let connector = LocalMasque::available();
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectUdp], None::<[MasqueService; 0]>).unwrap(),
    );

    let established = adapter
        .establish_data_plane(request(Transport::Udp, None))
        .await
        .unwrap();
    assert_eq!(established.negotiation.version, MasqueHttpVersion::Http3);
    let DataPlaneTunnel::Datagram(mut tunnel) = established.tunnel else {
        panic!("expected a datagram tunnel");
    };
    assert_eq!(tunnel.send(b"one".to_vec()).await.unwrap(), 3);
    assert_eq!(tunnel.send(b"two".to_vec()).await.unwrap(), 3);
    assert_eq!(tunnel.receive(32).await.unwrap(), b"one");
    assert_eq!(tunnel.receive(32).await.unwrap(), b"two");

    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].version, MasqueHttpVersion::Http3);
    assert_eq!(attempts[0].method, MasqueMethod::Connect);
    assert_eq!(attempts[0].service.protocol(), "connect-udp");
    assert_eq!(attempts[0].authority, "proxy.example");
    assert_eq!(attempts[0].scheme, "https");
    assert_eq!(attempts[0].path, "/.well-known/masque/udp/192.0.2.6/443/");
    assert!(attempts[0].uses_capsule_protocol());
}

#[tokio::test]
async fn local_connector_receives_rfc9484_connect_ip_and_preserves_packets() {
    let connector = LocalMasque::available();
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectIp], None::<[MasqueService; 0]>).unwrap(),
    );

    let established = adapter
        .establish_data_plane(request(Transport::Ip, None))
        .await
        .unwrap();
    let DataPlaneTunnel::Packet(mut tunnel) = established.tunnel else {
        panic!("expected a packet tunnel");
    };
    assert_eq!(tunnel.send(vec![0x45, 0, 0, 20]).await.unwrap(), 4);
    assert_eq!(tunnel.receive(32).await.unwrap(), vec![0x45, 0, 0, 20]);

    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts[0].service.protocol(), "connect-ip");
    assert_eq!(attempts[0].path, "/.well-known/masque/ip/*/*");
    assert!(attempts[0].uses_capsule_protocol());
}

#[tokio::test]
async fn http2_fallback_requires_pre_request_h3_failure_and_advertisement() {
    let connector = LocalMasque::with_h3_failure(MasqueAttemptFailure::BeforeRequest);
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new(
            [MasqueService::ConnectUdp],
            Some([MasqueService::ConnectUdp]),
        )
        .unwrap(),
    );

    let established = adapter
        .establish_data_plane(request(Transport::Udp, None))
        .await
        .unwrap();
    assert_eq!(
        established.negotiation.version,
        MasqueHttpVersion::Http2Fallback
    );
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].version, MasqueHttpVersion::Http3);
    assert_eq!(attempts[1].version, MasqueHttpVersion::Http2Fallback);
}

#[tokio::test]
async fn rejected_h3_and_missing_credentials_fail_closed_without_fallback() {
    let connector = LocalMasque::with_h3_failure(MasqueAttemptFailure::Rejected);
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new(
            [MasqueService::ConnectUdp],
            Some([MasqueService::ConnectUdp]),
        )
        .unwrap(),
    );

    assert!(
        adapter
            .establish_data_plane(request(Transport::Udp, None))
            .await
            .is_err()
    );
    assert_eq!(attempts.lock().unwrap().len(), 1);

    let connector = LocalMasque::available();
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectUdp], None::<[MasqueService; 0]>).unwrap(),
    );
    assert!(
        adapter
            .establish_data_plane(request(Transport::Udp, Some(CredentialRef(7))))
            .await
            .is_err()
    );
    assert!(attempts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn resolved_credentials_are_available_only_to_the_adapter_connector() {
    let connector = LocalMasque::available();
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(Some(MasqueCredentials::bearer("token"))),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectUdp], None::<[MasqueService; 0]>).unwrap(),
    );

    adapter
        .establish_data_plane(request(Transport::Udp, Some(CredentialRef(7))))
        .await
        .unwrap();
    let attempts = attempts.lock().unwrap();
    assert!(attempts[0].has_credentials());
    assert_eq!(attempts[0].bearer_token(), Some(b"token".as_slice()));
}

#[tokio::test]
async fn tcp_plan_is_rejected_before_a_masque_connection_attempt() {
    let connector = LocalMasque::available();
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectUdp], None::<[MasqueService; 0]>).unwrap(),
    );

    assert!(
        adapter
            .establish_data_plane(request(Transport::Tcp, None))
            .await
            .is_err()
    );
    assert!(attempts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn connect_ip_uses_an_explicit_http2_fallback_after_pre_request_h3_failure() {
    let connector = LocalMasque::with_h3_failure(MasqueAttemptFailure::BeforeRequest);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectIp], Some([MasqueService::ConnectIp]))
            .unwrap(),
    );

    let established = adapter
        .establish_data_plane(request(Transport::Ip, None))
        .await
        .unwrap();
    assert_eq!(
        established.negotiation,
        masque_adapter::NegotiatedMasque {
            version: MasqueHttpVersion::Http2Fallback,
            service: MasqueService::ConnectIp,
        }
    );
    assert!(matches!(established.tunnel, DataPlaneTunnel::Packet(_)));
}

#[tokio::test]
async fn h3_pre_request_failure_without_advertised_h2_fallback_fails_closed() {
    let connector = LocalMasque::with_h3_failure(MasqueAttemptFailure::BeforeRequest);
    let attempts = Arc::clone(&connector.attempts);
    let adapter = MasqueAdapter::new(
        Addresses,
        Credentials(None),
        connector,
        MasqueAdvertisement::new([MasqueService::ConnectUdp], None::<[MasqueService; 0]>).unwrap(),
    );

    assert!(
        adapter
            .establish_data_plane(request(Transport::Udp, None))
            .await
            .is_err()
    );
    assert_eq!(attempts.lock().unwrap().len(), 1);
}

#[test]
fn invalid_targets_are_not_constructible() {
    assert!(MasqueTarget::new("", "/path").is_none());
    assert!(MasqueTarget::new("https", "").is_none());
    assert!(MasqueTarget::new("http", "/path").is_none());
    assert!(MasqueTarget::new("https", "path").is_none());
    assert_eq!(
        MasqueAdvertisement::new(
            [MasqueService::ConnectUdp],
            Some([MasqueService::ConnectIp]),
        ),
        Err(MasqueConfigurationError::Http2FallbackWithoutHttp3)
    );
}
