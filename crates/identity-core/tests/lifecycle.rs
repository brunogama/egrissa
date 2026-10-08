use egress_core::{
    error::{LifecycleError, RecoveryError},
    ids::*,
    policy::RecoveryPolicy,
};
use identity_core::{
    continuity::IdentityContinuity,
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use std::time::{Duration, Instant};

#[test]
fn identity_transitions_expire_at_the_boundary_and_reject_stale_feedback() {
    let now = Instant::now();
    let end = now + Duration::from_secs(10);
    let mut identity = NetworkIdentity::allocate(
        NetworkIdentityId(1),
        WorkloadId(1),
        IdentityScope::Workload,
        now,
        Some(end),
    )
    .unwrap();
    identity
        .transition(NetworkIdentityId(1), 0, IdentityState::Active, now)
        .unwrap();
    assert_eq!(
        identity.transition(NetworkIdentityId(1), 1, IdentityState::Degraded, now),
        Err(LifecycleError::StaleFeedback)
    );
    assert_eq!(identity.state(), IdentityState::Active);
    assert!(!identity.is_usable(end));
    assert_eq!(
        identity.transition(NetworkIdentityId(1), 0, IdentityState::Degraded, end),
        Err(LifecycleError::Expired)
    );
    assert_eq!(identity.state(), IdentityState::Expired);
}

#[test]
fn replacement_is_explicit_and_unknown_never_becomes_preserved() {
    let now = Instant::now();
    let mut old = NetworkIdentity::allocate(
        NetworkIdentityId(1),
        WorkloadId(1),
        IdentityScope::Workload,
        now,
        None,
    )
    .unwrap();
    old.transition(old.id(), 0, IdentityState::Active, now)
        .unwrap();
    let original = old.clone();
    assert_eq!(
        old.replace(
            RecoveryPolicy::FailClosed,
            NetworkIdentityId(2),
            IdentityContinuity::Changed,
            now,
            None
        ),
        Err(RecoveryError::PolicyDenied)
    );
    assert_eq!(
        old.replace(
            RecoveryPolicy::ReplanAllowingIdentityChange,
            NetworkIdentityId(2),
            IdentityContinuity::Preserved,
            now,
            None
        ),
        Err(RecoveryError::InvalidContinuity)
    );
    assert_eq!(old, original);
    let replacement = old
        .replace(
            RecoveryPolicy::ReplanAllowingIdentityChange,
            NetworkIdentityId(2),
            IdentityContinuity::Unknown,
            now,
            None,
        )
        .unwrap();
    assert_eq!(replacement.continuity, IdentityContinuity::Unknown);
    assert_eq!(replacement.new.generation(), 1);
    assert_eq!(old.state(), IdentityState::Invalidated);
}
