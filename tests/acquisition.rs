mod support;
use egress_core::{error::*, ids::*, policy::*};
use route_core::{lifecycle::*, planner::*};
use std::{
    sync::{Arc, Barrier, Mutex},
    thread,
    time::{Duration, Instant},
};
use support::*;

#[test]
fn repeated_compatible_acquisition_reuses_route_and_identity() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let ctx = context(&req, &policy, &id, &inv, &health, now);
    let mut manager = RouteManager::default();
    let AcquireDecision::Establish(plan) = manager.acquire(&ctx, &mut FirstCandidate) else {
        panic!("expected establishment");
    };
    assert_eq!(manager.state(plan.id()), Some(RouteState::Establishing));
    assert_eq!(
        manager.acquire(&ctx, &mut FirstCandidate),
        AcquireDecision::Pending {
            route: plan.id(),
            route_generation: 0
        }
    );
    manager
        .complete(plan.key(), EstablishmentResult::Established, now)
        .unwrap();
    let mut next = req.clone();
    next.id = RequestId(2);
    let AcquireDecision::Reuse(lease) = manager.acquire(
        &context(&next, &policy, &id, &inv, &health, now),
        &mut FirstCandidate,
    ) else {
        panic!("expected reuse");
    };
    assert_eq!(lease.plan, plan);
    assert_eq!(
        manager.complete(plan.key(), EstablishmentResult::Established, now),
        Err(LifecycleError::InvalidTransition)
    );
    assert!(!lease.plan.is_valid(now + Duration::from_secs(60)));
}
#[test]
fn concurrent_acquisitions_permit_one_establishment_only() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let manager = Arc::new(Mutex::new(RouteManager::default()));
    let barrier = Arc::new(Barrier::new(8));
    thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                let ctx = context(&req, &policy, &id, &inv, &health, now);
                scope.spawn(move || {
                    barrier.wait();
                    manager.lock().unwrap().acquire(&ctx, &mut FirstCandidate)
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, AcquireDecision::Establish(_)))
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, AcquireDecision::Pending { .. }))
                .count(),
            7
        );
    });
}
#[test]
fn stale_route_or_identity_feedback_never_mutates_the_route() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    let AcquireDecision::Establish(plan) = manager.acquire(
        &context(&req, &policy, &id, &inv, &health, now),
        &mut FirstCandidate,
    ) else {
        panic!("expected establishment");
    };
    for reason in 0..4 {
        let mut key = plan.key();
        match reason {
            0 => key.route_generation += 1,
            1 => key.identity_generation += 1,
            2 => key.route = RouteId(99),
            _ => key.identity = NetworkIdentityId(99),
        }
        assert_eq!(
            manager.complete(key, EstablishmentResult::Failed, now),
            Err(LifecycleError::StaleFeedback)
        );
        assert_eq!(manager.state(plan.id()), Some(RouteState::Establishing));
    }
}

#[test]
fn route_replacement_changes_only_the_route_generation_and_rejects_old_feedback() {
    use identity_core::continuity::IdentityContinuity;
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy {
        recovery: RecoveryPolicy::ReplanPreservingIdentity,
        ..Default::default()
    };
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    let mut ctx = context(&req, &policy, &id, &inv, &health, now);
    let AcquireDecision::Establish(old) = manager.acquire(&ctx, &mut FirstCandidate) else {
        panic!("expected establishment");
    };
    manager
        .complete(old.key(), EstablishmentResult::Failed, now)
        .unwrap();
    ctx.route_generation = 1;
    assert_eq!(
        manager.recover(&ctx, IdentityContinuity::Unknown, &mut FirstCandidate),
        ReplacementDecision::Reject(RecoveryError::ContinuityUnproven)
    );
    let ReplacementDecision::PreserveIdentity(new) =
        manager.recover(&ctx, IdentityContinuity::Preserved, &mut FirstCandidate)
    else {
        panic!("expected preserving replan");
    };
    assert_eq!(new.identity_generation(), 0);
    assert_eq!(new.route_generation(), 1);
    assert_eq!(
        manager.complete(old.key(), EstablishmentResult::Established, now),
        Err(LifecycleError::StaleFeedback)
    );
    assert_eq!(manager.state(new.id()), Some(RouteState::Establishing));
}
#[test]
fn observations_do_not_become_establishment_health_or_verification() {
    use egress_core::evidence::*;
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    let AcquireDecision::Establish(plan) = manager.acquire(
        &context(&req, &policy, &id, &inv, &health, now),
        &mut FirstCandidate,
    ) else {
        panic!("expected establishment");
    };
    for (n, outcome) in [
        Outcome::HttpStatus(403),
        Outcome::Established,
        Outcome::VerificationPassed(VerificationId(1)),
        Outcome::VerificationFailed(VerificationId(2)),
    ]
    .into_iter()
    .enumerate()
    {
        manager
            .record_observation(Observation {
                id: ObservationId(n as u128),
                key: plan.key(),
                observed_at: now,
                scope: ObservationScope::Route(plan.id()),
                outcome,
            })
            .unwrap();
        assert_eq!(manager.state(plan.id()), Some(RouteState::Establishing));
    }
    assert_eq!(manager.observations().len(), 4);
}
#[test]
fn expired_routes_and_changed_inventory_are_not_reused() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let inv = inventory(now);
    let health = ObservationSnapshot::default();
    let mut manager = RouteManager::default();
    let AcquireDecision::Establish(plan) = manager.acquire(
        &context(&req, &policy, &id, &inv, &health, now),
        &mut FirstCandidate,
    ) else {
        panic!("expected establishment");
    };
    manager
        .complete(plan.key(), EstablishmentResult::Established, now)
        .unwrap();
    let mut disabled = inv.clone();
    disabled.endpoints[0].enabled = false;
    assert!(matches!(
        manager.acquire(
            &context(&req, &policy, &id, &disabled, &health, now),
            &mut FirstCandidate
        ),
        AcquireDecision::Reject(_)
    ));
    assert!(matches!(
        manager.acquire(
            &context(&req, &policy, &id, &inv, &health, plan.expires_at()),
            &mut FirstCandidate
        ),
        AcquireDecision::Reject(_)
    ));
    assert_eq!(manager.state(plan.id()), Some(RouteState::Expired));
}

/// Stale or skipped identity generations cannot authorize recovery, and a rejected recovery
/// leaves the current route state and retry budget untouched.
#[test]
fn stale_or_ahead_identity_generations_cannot_mutate_the_route() {
    use identity_core::{continuity::IdentityContinuity, lifecycle::IdentityState};
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy {
        recovery: RecoveryPolicy::ReplanAllowingIdentityChange,
        ..Default::default()
    };
    let mut id = identity(now);
    let mut spare = id.clone();
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
    let intermediate = id
        .replace(
            policy.recovery,
            NetworkIdentityId(2),
            IdentityContinuity::Unknown,
            now,
            None,
        )
        .unwrap();
    let mut ahead = intermediate.new;
    ahead
        .transition(ahead.id(), ahead.generation(), IdentityState::Active, now)
        .unwrap();
    let beyond = ahead
        .replace(
            policy.recovery,
            NetworkIdentityId(3),
            IdentityContinuity::Unknown,
            now,
            None,
        )
        .unwrap();
    let mut ahead = beyond.new;
    ahead
        .transition(ahead.id(), ahead.generation(), IdentityState::Active, now)
        .unwrap();
    assert_eq!(ahead.generation(), 2);
    let spare_replacement = spare
        .replace(
            policy.recovery,
            NetworkIdentityId(2),
            IdentityContinuity::Unknown,
            now,
            None,
        )
        .unwrap();
    let mut next = spare_replacement.new;
    next.transition(next.id(), next.generation(), IdentityState::Active, now)
        .unwrap();
    assert_eq!(next.generation(), 1);
    let mut ahead_context = context(&req, &policy, &ahead, &inv, &health, now);
    ahead_context.route_generation = 1;
    assert_eq!(
        manager.recover(
            &ahead_context,
            IdentityContinuity::Unknown,
            &mut FirstCandidate
        ),
        ReplacementDecision::Reject(RecoveryError::Lifecycle(LifecycleError::StaleFeedback))
    );
    assert_eq!(manager.state(old.id()), Some(RouteState::Failed));
    let mut unknown_route = context(&req, &policy, &next, &inv, &health, now);
    unknown_route.route = RouteId(99);
    unknown_route.route_generation = 1;
    assert_eq!(
        manager.recover(
            &unknown_route,
            IdentityContinuity::Unknown,
            &mut FirstCandidate
        ),
        ReplacementDecision::Reject(RecoveryError::Lifecycle(LifecycleError::StaleFeedback))
    );
    assert_eq!(manager.state(old.id()), Some(RouteState::Failed));
    let mut legitimate = context(&req, &policy, &next, &inv, &health, now);
    legitimate.route_generation = 1;
    assert!(matches!(
        manager.recover(
            &legitimate,
            IdentityContinuity::Unknown,
            &mut FirstCandidate
        ),
        ReplacementDecision::ReplaceIdentity { .. }
    ));
    assert_eq!(manager.state(old.id()), Some(RouteState::Establishing));
}
