use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{EndpointId, NetworkIdentityId, OperatorId, RequestId, RouteId, WorkloadId},
    policy::{EgressPolicy, RecoveryPolicy},
    request::{
        Destination, DestinationKey, EgressRequest, GeographyRequirement, IdentityRequirement,
        IsolationRequirement, PrivacyRequirement, TransportRequirement,
    },
};
use egress_runtime::{
    AcquireContext, Clock, EgressRuntime, RuntimeError, RuntimeEvent,
    fake::{FakeTransport, ScriptedStep},
};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use observation_core::{EgressObservation, ObservationScope, Outcome};
use route_core::{
    planner::{Candidate, CredentialLease, DestinationEligibility, Endpoint, InventorySnapshot},
    topology::RouteTopology,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use transport_core::TransportFailure;
use verification_core::{
    EgressVerifier, EvidenceVerifier, VerificationFailure, VerificationFuture, VerificationOutcome,
    VerificationSubject,
};

#[derive(Clone, Copy)]
struct TestClock(Instant);
impl Clock for TestClock {
    fn now(&self) -> Instant {
        self.0
    }
}

fn context(now: Instant) -> AcquireContext {
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
            credential: None,
        }],
        upstreams: vec![],
        candidates: vec![Candidate {
            topology: RouteTopology::Relay {
                relay: EndpointId(1),
            },
            rank: 0,
        }],
        credentials: vec![CredentialLease {
            reference: egress_core::ids::CredentialRef(1),
            valid_until: None,
        }],
    };
    AcquireContext {
        request: EgressRequest {
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
        },
        policy: EgressPolicy::default(),
        identity,
        inventory,
        observations: Default::default(),
        route: RouteId(1),
        route_generation: 0,
        now,
        egress_observations: Vec::new(),
    }
}

async fn wait_until_waiting(fake: &FakeTransport, target: usize) {
    for _ in 0..1000 {
        if fake.waiting() == target {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("fake transport did not reach the requested gate state");
}

#[tokio::test]
async fn m1_01_successful_end_to_end_acquire() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
    let lease = runtime.acquire(context(now)).await.unwrap();
    assert_eq!(lease.lease.plan.expires_at(), now + Duration::from_secs(60));
}

#[tokio::test]
async fn m1_02_establishment_observation_is_recorded() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
    runtime.acquire(context(now)).await.unwrap();
    let observations = runtime.observations();
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0].outcome, Outcome::EstablishmentStarted);
    assert_eq!(observations[1].outcome, Outcome::Established);
    assert_eq!(
        observations[1].scope,
        ObservationScope::Endpoint(EndpointId(1))
    );
}

#[tokio::test]
async fn m1_03_verification_is_distinct_from_establishment() {
    let now = Instant::now();
    let mut input = context(now);
    input.policy.verification = egress_core::policy::VerificationPolicy::RequireBeforeUse;
    input.egress_observations = vec![EgressObservation {
        exit_address: Some("203.0.113.7".parse().unwrap()),
        ..Default::default()
    }];
    let runtime = EgressRuntime::with_verifier(
        FakeTransport::new([ScriptedStep::Succeed]),
        TestClock(now),
        Arc::new(EvidenceVerifier),
    );
    let lease = runtime.acquire(input).await.unwrap();
    assert!(lease.verification.is_some());
    assert!(
        runtime
            .observations()
            .iter()
            .any(|item| item.outcome == Outcome::Established)
    );
    assert!(
        runtime
            .observations()
            .iter()
            .any(|item| matches!(item.outcome, Outcome::VerificationPassed(_)))
    );
}

struct FailingVerifier;
impl EgressVerifier for FailingVerifier {
    fn verify(&self, _subject: VerificationSubject) -> VerificationFuture {
        Box::pin(async move { VerificationOutcome::Failed(VerificationFailure::Contradicted) })
    }
}

struct StaleVerifier;
impl EgressVerifier for StaleVerifier {
    fn verify(&self, subject: VerificationSubject) -> VerificationFuture {
        Box::pin(async move {
            let mut key = subject.plan.key();
            key.identity_generation += 1;
            VerificationOutcome::Verified(verification_core::VerifiedEgress {
                id: subject.id,
                key,
                verified_at: subject.now,
                properties: verification_core::VerifiedProperties::default(),
                evidence: vec![egress_core::ids::ObservationId(1)],
            })
        })
    }
}

#[tokio::test]
async fn m1_04_verification_failure_does_not_rewrite_establishment_history() {
    let now = Instant::now();
    let runtime = EgressRuntime::with_verifier(
        FakeTransport::new([ScriptedStep::Succeed]),
        TestClock(now),
        Arc::new(FailingVerifier),
    );
    let acquired = runtime.acquire(context(now)).await.unwrap();
    assert!(acquired.verification.is_none());
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
    let observations = runtime.observations();
    assert!(
        observations
            .iter()
            .any(|item| item.outcome == Outcome::Established)
    );
    assert!(
        observations
            .iter()
            .any(|item| matches!(item.outcome, Outcome::VerificationFailed(_)))
    );
}

#[tokio::test]
async fn m1_13_identity_changing_replacement_increments_identity_generation() {
    let now = Instant::now();
    let mut input = context(now);
    input.policy.recovery = RecoveryPolicy::ReplanAllowingIdentityChange;
    let fake = FakeTransport::new([
        ScriptedStep::Fail(TransportFailure::ConnectionReset),
        ScriptedStep::Succeed,
    ]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let lease = runtime.acquire(input).await.unwrap();
    assert_eq!(lease.lease.plan.route_generation(), 1);
    assert_eq!(lease.lease.plan.identity_generation(), 1);
    assert_eq!(lease.lease.plan.identity(), NetworkIdentityId(2));
    let establishment_keys = fake.establishments();
    assert_eq!(establishment_keys.len(), 2);
    assert_eq!(
        runtime.complete_feedback(
            establishment_keys[0],
            route_core::lifecycle::EstablishmentResult::Established
        ),
        Err(RuntimeError::Lifecycle(
            egress_core::error::LifecycleError::StaleFeedback
        ))
    );
    assert_eq!(
        runtime.complete_feedback(
            establishment_keys[0],
            route_core::lifecycle::EstablishmentResult::Failed
        ),
        Err(RuntimeError::Lifecycle(
            egress_core::error::LifecycleError::StaleFeedback
        ))
    );
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
}

#[tokio::test]
async fn unsupported_topology_is_not_misreported_as_connection_reset() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(
        FakeTransport::new([ScriptedStep::Fail(TransportFailure::UnsupportedTopology)]),
        TestClock(now),
    );
    assert!(runtime.acquire(context(now)).await.is_err());
    assert!(
        runtime
            .observations()
            .iter()
            .any(|observation| observation.outcome == Outcome::UnsupportedTopology)
    );
    assert!(
        !runtime
            .observations()
            .iter()
            .any(|observation| observation.outcome == Outcome::ConnectionReset)
    );
}

#[tokio::test]
async fn m1_15_fail_closed_never_issues_a_lease() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(
        FakeTransport::new([ScriptedStep::Fail(TransportFailure::ConnectTimeout)]),
        TestClock(now),
    );
    assert!(matches!(
        runtime.acquire(context(now)).await,
        Err(RuntimeError::Establishment(
            TransportFailure::ConnectTimeout
        ))
    ));
    assert!(
        runtime
            .events()
            .iter()
            .all(|event| !matches!(event, RuntimeEvent::LeaseIssued { .. }))
    );
}

#[tokio::test]
async fn m1_16_retry_limit_is_respected_without_continuity_evidence() {
    let now = Instant::now();
    let mut input = context(now);
    input.policy.recovery = RecoveryPolicy::RetrySameRoute { attempts: 1 };
    let fake = FakeTransport::new([ScriptedStep::Fail(TransportFailure::ConnectionReset)]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    assert!(runtime.acquire(input).await.is_err());
    assert_eq!(fake.establishments().len(), 1);
}

#[tokio::test]
async fn m1_17_expiry_prevents_a_new_lease() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
    let lease = runtime.acquire(context(now)).await.unwrap();
    let mut expired = context(lease.lease.plan.expires_at());
    expired.identity = context(now).identity;
    assert!(runtime.acquire(expired).await.is_err());
    assert!(
        runtime
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::RouteExpired { route: RouteId(1) }))
    );
}

#[tokio::test]
async fn m1_18_existing_lease_exposes_expiry_without_mid_use_revocation() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
    let lease = runtime.acquire(context(now)).await.unwrap();
    assert_eq!(lease.lease.plan.expires_at(), now + Duration::from_secs(60));
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
}

#[tokio::test]
async fn m1_24_duplicate_and_stale_callbacks_are_rejected() {
    let now = Instant::now();
    let runtime = EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
    let lease = runtime.acquire(context(now)).await.unwrap();
    let key = lease.lease.plan.key();
    assert_eq!(
        runtime.complete_feedback(key, route_core::lifecycle::EstablishmentResult::Established),
        Err(RuntimeError::Lifecycle(
            egress_core::error::LifecycleError::InvalidTransition
        ))
    );
    let mut stale = key;
    stale.route_generation += 1;
    assert_eq!(
        runtime.complete_feedback(stale, route_core::lifecycle::EstablishmentResult::Failed),
        Err(RuntimeError::Lifecycle(
            egress_core::error::LifecycleError::StaleFeedback
        ))
    );
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
}

#[tokio::test]
async fn stale_verification_cannot_mutate_the_current_route() {
    let now = Instant::now();
    let mut input = context(now);
    input.policy.verification = egress_core::policy::VerificationPolicy::RequireBeforeUse;
    let runtime = EgressRuntime::with_verifier(
        FakeTransport::new([ScriptedStep::Succeed]),
        TestClock(now),
        Arc::new(StaleVerifier),
    );
    assert_eq!(
        runtime.acquire(input).await,
        Err(RuntimeError::VerificationUnavailable)
    );
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
    assert!(
        !runtime
            .observations()
            .iter()
            .any(|observation| matches!(observation.outcome, Outcome::VerificationPassed(_)))
    );
}

#[tokio::test]
async fn m1_05_one_hundred_acquires_share_one_establishment() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Succeed))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let mut tasks = Vec::new();
    for _ in 0..100 {
        let runtime = runtime.clone();
        let context = context(now);
        tasks.push(tokio::spawn(async move { runtime.acquire(context).await }));
    }
    wait_until_waiting(&fake, 1).await;
    assert_eq!(fake.establishments().len(), 1);
    fake.release(1);
    for task in tasks {
        task.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn m1_06_pending_callers_wake_on_success() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Succeed))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let first = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    wait_until_waiting(&fake, 1).await;
    let second = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    tokio::task::yield_now().await;
    fake.release(1);
    let a = first.await.unwrap().unwrap();
    let b = second.await.unwrap().unwrap();
    assert_eq!(a.lease.plan.key(), b.lease.plan.key());
}

#[tokio::test]
async fn m1_07_pending_callers_wake_on_failure() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Fail(
        TransportFailure::ConnectTimeout,
    )))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let first = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    wait_until_waiting(&fake, 1).await;
    let second = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    tokio::task::yield_now().await;
    fake.release(1);
    assert!(matches!(
        first.await.unwrap(),
        Err(RuntimeError::Establishment(
            TransportFailure::ConnectTimeout
        ))
    ));
    assert!(matches!(
        second.await.unwrap(),
        Err(RuntimeError::Establishment(
            TransportFailure::ConnectTimeout
        ))
    ));
}

#[tokio::test]
async fn m1_08_control_plane_is_available_while_transport_is_suspended() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Succeed))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    wait_until_waiting(&fake, 1).await;
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Establishing)
    );
    fake.release(1);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn m1_21_fake_script_is_reproducible() {
    let now = Instant::now();
    let run = async {
        let runtime =
            EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
        runtime.acquire(context(now)).await.unwrap();
        (runtime.events(), runtime.observations())
    };
    let first = run.await;
    let second_runtime =
        EgressRuntime::new(FakeTransport::new([ScriptedStep::Succeed]), TestClock(now));
    second_runtime.acquire(context(now)).await.unwrap();
    assert_eq!(
        first,
        (second_runtime.events(), second_runtime.observations())
    );
}

#[tokio::test]
async fn fake_establish_then_fail_reports_a_session_failure_separately() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::SucceedThenFail(
        TransportFailure::ConnectionReset,
    )]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let lease = runtime.acquire(context(now)).await.unwrap();
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
    assert_eq!(
        fake.pending_session_failures(),
        vec![(lease.lease.plan.key(), TransportFailure::ConnectionReset)]
    );
}

#[tokio::test]
async fn m1_22_waiter_cancellation_does_not_corrupt_state() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Succeed))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let first = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    wait_until_waiting(&fake, 1).await;
    let waiter = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    tokio::task::yield_now().await;
    waiter.abort();
    fake.release(1);
    first.await.unwrap().unwrap();
    assert_eq!(
        runtime.state(RouteId(1)),
        Some(route_core::lifecycle::RouteState::Ready)
    );
}

#[tokio::test]
async fn m1_23_owner_cancellation_cannot_leave_establishing() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Succeed))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let owner = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    wait_until_waiting(&fake, 1).await;
    owner.abort();
    fake.release(1);
    for _ in 0..1000 {
        if runtime.state(RouteId(1)) == Some(route_core::lifecycle::RouteState::Ready) {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("detached establishment did not complete after owner cancellation");
}

#[tokio::test]
async fn m1_25_shutdown_resolves_pending_waiters() {
    let now = Instant::now();
    let fake = FakeTransport::new([ScriptedStep::AwaitRelease(Box::new(ScriptedStep::Succeed))]);
    let runtime = EgressRuntime::new(fake.clone(), TestClock(now));
    let owner = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    wait_until_waiting(&fake, 1).await;
    let waiter = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.acquire(context(now)).await }
    });
    tokio::task::yield_now().await;
    runtime.shutdown();
    assert_eq!(owner.await.unwrap(), Err(RuntimeError::Shutdown));
    assert_eq!(waiter.await.unwrap(), Err(RuntimeError::Shutdown));
}
