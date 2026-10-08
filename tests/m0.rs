mod support;
use egress_core::{capability::*, error::*, ids::*, policy::*};
use route_core::planner::*;
use std::time::{Duration, Instant};
use support::*;
#[test]
fn no_candidate_returns_typed_failure_not_direct_fallback() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = InventorySnapshot::default();
    let health = ObservationSnapshot::default();
    assert_eq!(
        DeterministicPlanner::default().plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
}

#[test]
fn eligible_planning_is_deterministic_and_bounded_by_dependency_validity() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let ctx = context(&req, &policy, &id, &inv, &health, now);
    let planner = DeterministicPlanner::default();
    let plan = planner.plan(&ctx, &mut FirstCandidate).unwrap();
    assert_eq!(plan.expires_at(), now + Duration::from_secs(60));
    assert!(!plan.is_valid(plan.expires_at()));
    assert_eq!(plan.identity(), id.id());
    assert_eq!(plan.credentials(), &[CredentialRef(1)]);
    assert_eq!(planner.plan(&ctx, &mut FirstCandidate), Ok(plan));
}

#[test]
fn planner_rejects_unsupported_topology_and_ineligible_dependencies() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let health = ObservationSnapshot::default();
    let inv = inventory(now);
    let unsupported = DeterministicPlanner {
        supported_topologies: vec![TopologyKind::Socks5],
    };
    assert_eq!(
        unsupported.plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        Err(PlanningError::NoEligibleCandidate)
    );
    for reason in 0..5 {
        let mut inv = inventory(now);
        match reason {
            0 => inv.endpoints[0].enabled = false,
            1 => inv.endpoints[0].available = false,
            2 => inv.endpoints[0].destinations = DestinationEligibility::Only(vec![]),
            3 => inv.endpoints[0].capabilities.topology.supported.clear(),
            _ => inv.endpoints[0].capabilities.isolation.shared = false,
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
