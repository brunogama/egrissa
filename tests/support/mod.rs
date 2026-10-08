use egress_core::{capability::*, ids::*, policy::*, request::*};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
pub use observation_core::ObservationSnapshot;
use route_core::{planner::*, topology::RouteTopology};
use std::time::{Duration, Instant};

pub fn request() -> EgressRequest {
    EgressRequest {
        id: RequestId(1),
        workload: WorkloadId(1),
        destination: Destination {
            key: DestinationKey(1),
            port: 443,
        },
        identity: IdentityRequirement::Ephemeral,
        transport: TransportRequirement::Tcp,
        geography: GeographyRequirement::Any,
        isolation: IsolationRequirement::Shared,
        privacy: PrivacyRequirement::Standard,
    }
}
pub fn identity(now: Instant) -> NetworkIdentity {
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
    identity
}
pub fn capabilities() -> EgressCapabilities {
    EgressCapabilities {
        transport: TransportCapabilities {
            tcp: true,
            udp: true,
            ..Default::default()
        },
        identity: IdentityCapabilities {
            sticky: true,
            maximum_sticky_ttl: Some(Duration::from_secs(120)),
            continuity_observable: true,
        },
        topology: TopologyCapabilities {
            supported: vec![TopologyKind::Relay],
            separate_client_and_destination: true,
        },
        isolation: IsolationCapabilities {
            shared: true,
            per_session: true,
            per_workload: true,
            dedicated: true,
        },
        ..Default::default()
    }
}
pub fn inventory(now: Instant) -> InventorySnapshot {
    InventorySnapshot {
        endpoints: vec![Endpoint {
            id: EndpointId(1),
            operator: OperatorId(1),
            enabled: true,
            available: true,
            destinations: DestinationEligibility::Any,
            capabilities: capabilities(),
            valid_until: Some(now + Duration::from_secs(60)),
            credential: Some(CredentialRef(1)),
        }],
        upstreams: vec![],
        candidates: vec![Candidate {
            topology: RouteTopology::Relay {
                relay: EndpointId(1),
            },
            rank: 0,
        }],
        credentials: vec![
            CredentialLease {
                reference: CredentialRef(1),
                valid_until: None,
            },
            CredentialLease {
                reference: CredentialRef(2),
                valid_until: None,
            },
        ],
    }
}
pub fn context<'a>(
    request: &'a EgressRequest,
    policy: &'a EgressPolicy,
    identity: &'a NetworkIdentity,
    inventory: &'a InventorySnapshot,
    health: &'a ObservationSnapshot,
    now: Instant,
) -> PlanContext<'a> {
    PlanContext {
        request,
        policy,
        identity,
        inventory,
        observations: health,
        now,
        route: RouteId(1),
        route_generation: 0,
    }
}
