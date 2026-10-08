mod support;
use egress_core::{capability::*, error::*, ids::*, policy::*, request::*};
use route_core::{planner::*, topology::RouteTopology};
use std::time::{Duration, Instant};
use support::*;

#[test]
fn identity_requirements_are_checked_before_planning() {
    let now = Instant::now();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut req = request();
    req.identity = IdentityRequirement::Existing(NetworkIdentityId(99));
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::IncompatibleIdentity)
    );
    req.identity = IdentityRequirement::Sticky {
        minimum_lifetime: Duration::from_secs(61),
    };
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
}
#[test]
fn geography_health_and_sticky_claims_filter_candidates() {
    let now = Instant::now();
    let policy = EgressPolicy::default();
    let id = identity(now);
    for reason in 0..5 {
        let mut req = request();
        let mut inv = inventory(now);
        let mut health = ObservationSnapshot::default();
        match reason {
            0 => req.geography = GeographyRequirement::Country(CountryCode(*b"GB")),
            1 => health.unavailable_endpoints.push(EndpointId(1)),
            2 => health
                .endpoint_cooldowns
                .push((EndpointId(1), now + Duration::from_secs(1))),
            3 => {
                req.identity = IdentityRequirement::Sticky {
                    minimum_lifetime: Duration::from_secs(30),
                };
                inv.endpoints[0].capabilities.identity.sticky = false;
            }
            _ => {
                req.identity = IdentityRequirement::Sticky {
                    minimum_lifetime: Duration::from_secs(30),
                };
                inv.endpoints[0].capabilities.identity.maximum_sticky_ttl =
                    Some(Duration::from_secs(29));
            }
        }
        assert_eq!(
            DeterministicPlanner::default().plan(
                &context(&req, &policy, &id, &inv, &health, now),
                &mut FirstCandidate
            ),
            Err(PlanningError::NoEligibleCandidate),
            "reason {reason}"
        );
    }
}
#[test]
fn privacy_cannot_be_claimed_for_a_single_relay() {
    let now = Instant::now();
    let mut req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    req.privacy = PrivacyRequirement::SeparateClientAndDestinationKnowledge;
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
}
#[test]
fn chooser_is_bounded_to_best_rank_equivalent_candidates() {
    struct BadChooser;
    impl CandidateChooser for BadChooser {
        fn choose(&mut self, count: usize) -> usize {
            count
        }
    }
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut BadChooser
        ),
        Err(PlanningError::InvalidChoice { index: 1, count: 1 })
    );
}

/// Explicit inventory shapes, no networking or arbitrary hop arrays.
fn chained_inventory(now: Instant) -> InventorySnapshot {
    let mut inv = inventory(now);
    let mut gateway = inv.endpoints[0].clone();
    gateway.id = EndpointId(2);
    gateway.operator = OperatorId(2);
    gateway.capabilities.topology.supported = vec![TopologyKind::Socks5];
    inv.endpoints.push(gateway);
    inv.upstreams.push(UpstreamBinding {
        id: UpstreamBindingId(1),
        operator: OperatorId(3),
        provider: ProviderId(1),
        available: true,
        destinations: DestinationEligibility::Any,
        capabilities: capabilities(),
        valid_until: Some(now + Duration::from_secs(40)),
        credential: Some(CredentialRef(2)),
    });
    inv.candidates = vec![Candidate {
        topology: RouteTopology::RelayThenSocks5 {
            relay: EndpointId(1),
            gateway: EndpointId(2),
            upstream: UpstreamBindingId(1),
        },
        rank: 0,
    }];
    inv
}
#[test]
fn distinct_operators_and_upstream_dependencies_are_required() {
    let now = Instant::now();
    let mut req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let health = ObservationSnapshot::default();
    req.privacy = PrivacyRequirement::DistinctOperators;
    let mut inv = chained_inventory(now);
    assert!(
        DeterministicPlanner::default()
            .plan(
                &context(&req, &policy, &id, &inv, &health, now),
                &mut FirstCandidate
            )
            .is_ok()
    );
    inv.endpoints[1].operator = OperatorId(1);
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
    inv.endpoints[1].operator = OperatorId(2);
    inv.upstreams[0].available = false;
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
}
#[test]
fn ip_over_socks_and_udp_over_http_connect_are_not_supported() {
    let now = Instant::now();
    let mut req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let health = ObservationSnapshot::default();
    let mut inv = chained_inventory(now);
    for endpoint in &mut inv.endpoints {
        endpoint.capabilities.transport.ip = true;
    }
    inv.upstreams[0].capabilities.transport.ip = true;
    req.transport = TransportRequirement::Ip;
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
    inv = inventory(now);
    inv.candidates[0].topology = RouteTopology::HttpConnect {
        proxy: EndpointId(1),
    };
    inv.endpoints[0].capabilities.topology.supported = vec![TopologyKind::HttpConnect];
    inv.endpoints[0].capabilities.transport.http_connect = true;
    req.transport = TransportRequirement::Udp;
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
}

#[test]
fn verification_requirements_and_ambiguous_inventory_fail_closed() {
    let now = Instant::now();
    let req = request();
    let mut policy = EgressPolicy::default();
    let id = identity(now);
    let mut inv = inventory(now);
    let health = ObservationSnapshot::default();
    policy.verification = VerificationPolicy::RequireBeforeUse;
    assert!(
        DeterministicPlanner::default()
            .plan(
                &context(&req, &policy, &id, &inv, &health, now),
                &mut FirstCandidate
            )
            .is_ok()
    );
    policy.verification = VerificationPolicy::ObserveOnly;
    policy.verification = VerificationPolicy::ObserveOnly;
    inv.endpoints.push(inv.endpoints[0].clone());
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::InvalidInventory)
    );
}
#[test]
fn credential_dependency_is_required_and_bounds_route_lifetime() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let mut inv = inventory(now);
    let health = ObservationSnapshot::default();
    inv.credentials[0].valid_until = Some(now + Duration::from_secs(7));
    let plan = DeterministicPlanner::default()
        .plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate,
        )
        .unwrap();
    assert_eq!(plan.expires_at(), now + Duration::from_secs(7));
    inv.credentials.clear();
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
}
