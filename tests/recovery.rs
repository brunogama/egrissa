mod support;
use egress_core::{error::*, ids::*, policy::*, request::*};
use identity_core::{
    continuity::IdentityContinuity,
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use route_core::{lifecycle::*, planner::*};
use std::time::Instant;
use support::*;

#[test]
fn identity_changing_recovery_is_explicit_and_rejects_preserved() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy {
        recovery: RecoveryPolicy::ReplanAllowingIdentityChange,
        ..Default::default()
    };
    let mut id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    let AcquireDecision::Establish(old) = manager.acquire(
        &context(&req, &policy, &id, &inv, &health, now),
        &mut FirstCandidate,
    ) else {
        panic!("expected establishment");
    };
    manager
        .complete(old.key(), EstablishmentResult::Failed, now)
        .unwrap();
    let replacement = id
        .replace(
            policy.recovery,
            NetworkIdentityId(2),
            IdentityContinuity::Unknown,
            now,
            None,
        )
        .unwrap();
    let mut new = replacement.new;
    new.transition(new.id(), new.generation(), IdentityState::Active, now)
        .unwrap();
    let mut ctx = context(&req, &policy, &new, &inv, &health, now);
    ctx.route_generation = 1;
    assert_eq!(
        manager.recover(&ctx, IdentityContinuity::Preserved, &mut FirstCandidate),
        ReplacementDecision::Reject(RecoveryError::InvalidContinuity)
    );
    let ReplacementDecision::ReplaceIdentity {
        old: old_id,
        new,
        route,
        continuity,
    } = manager.recover(&ctx, IdentityContinuity::Unknown, &mut FirstCandidate)
    else {
        panic!("expected explicit replacement");
    };
    assert_eq!(old_id, NetworkIdentityId(1));
    assert_eq!(new.generation(), 1);
    assert_eq!(continuity, IdentityContinuity::Unknown);
    assert_eq!(route.identity(), NetworkIdentityId(2));
    assert_eq!(
        manager.complete(old.key(), EstablishmentResult::Established, now),
        Err(LifecycleError::StaleFeedback)
    );
    assert_eq!(manager.state(route.id()), Some(RouteState::Establishing));
}
#[test]
fn fail_closed_and_retry_limits_cannot_be_bypassed_by_new_acquisitions() {
    let now = Instant::now();
    let req = request();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    for recovery in [
        RecoveryPolicy::FailClosed,
        RecoveryPolicy::RetrySameRoute { attempts: 1 },
    ] {
        let policy = EgressPolicy {
            recovery,
            ..Default::default()
        };
        let mut manager = RouteManager::default();
        let mut ctx = context(&req, &policy, &id, &inv, &health, now);
        let AcquireDecision::Establish(old) = manager.acquire(&ctx, &mut FirstCandidate) else {
            panic!("expected establishment");
        };
        manager
            .complete(old.key(), EstablishmentResult::Failed, now)
            .unwrap();
        ctx.route = RouteId(2);
        assert!(matches!(
            manager.acquire(&ctx, &mut FirstCandidate),
            AcquireDecision::Reject(_)
        ));
        ctx.route = RouteId(1);
        ctx.route_generation = 1;
        let decision = manager.recover(&ctx, IdentityContinuity::Preserved, &mut FirstCandidate);
        match recovery {
            RecoveryPolicy::FailClosed => assert_eq!(
                decision,
                ReplacementDecision::Reject(RecoveryError::PolicyDenied)
            ),
            _ => {
                let ReplacementDecision::PreserveIdentity(plan) = decision else {
                    panic!("expected one retry");
                };
                manager
                    .complete(plan.key(), EstablishmentResult::Failed, now)
                    .unwrap();
                ctx.route_generation = 2;
                assert_eq!(
                    manager.recover(&ctx, IdentityContinuity::Preserved, &mut FirstCandidate),
                    ReplacementDecision::Reject(RecoveryError::PolicyDenied)
                );
            }
        }
    }
}
#[test]
fn dedicated_identity_cannot_be_shared_by_distinct_requests() {
    let now = Instant::now();
    let mut req = request();
    req.identity = IdentityRequirement::Isolated;
    let policy = EgressPolicy::default();
    let mut id = NetworkIdentity::allocate(
        NetworkIdentityId(1),
        WorkloadId(1),
        IdentityScope::Dedicated,
        now,
        None,
    )
    .unwrap();
    id.transition(id.id(), 0, IdentityState::Active, now)
        .unwrap();
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    assert!(matches!(
        manager.acquire(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut FirstCandidate
        ),
        AcquireDecision::Establish(_)
    ));
    req.id = RequestId(2);
    let mut ctx = context(&req, &policy, &id, &inv, &health, now);
    ctx.route = RouteId(2);
    assert!(matches!(
        manager.acquire(&ctx, &mut FirstCandidate),
        AcquireDecision::Reject(_)
    ));
}
#[test]
fn logical_isolation_requires_adapter_isolation_too() {
    let now = Instant::now();
    let mut req = request();
    req.identity = IdentityRequirement::Isolated;
    let policy = EgressPolicy::default();
    let mut inv = inventory(now);
    inv.endpoints[0].capabilities.isolation.dedicated = false;
    let mut id = NetworkIdentity::allocate(
        NetworkIdentityId(1),
        WorkloadId(1),
        IdentityScope::Dedicated,
        now,
        None,
    )
    .unwrap();
    id.transition(id.id(), 0, IdentityState::Active, now)
        .unwrap();
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
fn old_identity_cannot_accept_multiple_replacements() {
    let now = Instant::now();
    let mut id = identity(now);
    id.replace(
        RecoveryPolicy::ReplanAllowingIdentityChange,
        NetworkIdentityId(2),
        IdentityContinuity::Changed,
        now,
        None,
    )
    .unwrap();
    assert_eq!(
        id.replace(
            RecoveryPolicy::ReplanAllowingIdentityChange,
            NetworkIdentityId(3),
            IdentityContinuity::Changed,
            now,
            None
        ),
        Err(RecoveryError::Lifecycle(LifecycleError::InvalidTransition))
    );
}

#[test]
fn retry_same_route_ignores_new_better_ranked_routes() {
    use route_core::topology::RouteTopology;
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy {
        recovery: RecoveryPolicy::RetrySameRoute { attempts: 1 },
        ..Default::default()
    };
    let id = identity(now);
    let mut inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    let AcquireDecision::Establish(old) = manager.acquire(
        &context(&req, &policy, &id, &inv, &health, now),
        &mut FirstCandidate,
    ) else {
        panic!("expected establishment");
    };
    manager
        .complete(old.key(), EstablishmentResult::Failed, now)
        .unwrap();
    inv.candidates[0].rank = 10;
    let mut endpoint = inv.endpoints[0].clone();
    endpoint.id = EndpointId(2);
    inv.endpoints.push(endpoint);
    inv.candidates.push(Candidate {
        topology: RouteTopology::Relay {
            relay: EndpointId(2),
        },
        rank: 0,
    });
    let mut ctx = context(&req, &policy, &id, &inv, &health, now);
    ctx.route_generation = 1;
    let ReplacementDecision::PreserveIdentity(retry) =
        manager.recover(&ctx, IdentityContinuity::Preserved, &mut FirstCandidate)
    else {
        panic!("expected same-route retry");
    };
    assert_eq!(retry.topology(), old.topology());
}
