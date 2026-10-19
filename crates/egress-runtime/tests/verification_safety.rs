use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{EndpointId, NetworkIdentityId, OperatorId, RequestId, RouteId, WorkloadId},
    policy::{EgressPolicy, VerificationPolicy},
    request::{
        Destination, DestinationKey, EgressRequest, GeographyRequirement, IdentityRequirement,
        IsolationRequirement, PrivacyRequirement, TransportRequirement,
    },
};
use egress_runtime::{
    AcquireContext, Clock, EgressRuntime, RuntimeError,
    fake::{FakeTransport, ScriptedStep},
};
use identity_core::{
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use observation_core::EgressObservation;
use route_core::{
    planner::{Candidate, DestinationEligibility, Endpoint, InventorySnapshot},
    topology::RouteTopology,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use verification_core::{
    EgressVerifier, VerificationFailure, VerificationFuture, VerificationOutcome,
    VerificationSubject, VerifiedEgress, VerifiedProperties,
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
        policy: EgressPolicy {
            verification: VerificationPolicy::RequireBeforeUse,
            ..Default::default()
        },
        identity,
        inventory: InventorySnapshot {
            endpoints: vec![Endpoint {
                id: EndpointId(1),
                operator: OperatorId(1),
                enabled: true,
                available: true,
                destinations: DestinationEligibility::Any,
                capabilities: EgressCapabilities {
                    transport: TransportCapabilities {
                        tcp: true,
                        ..Default::default()
                    },
                    identity: IdentityCapabilities {
                        sticky: true,
                        continuity_observable: true,
                        ..Default::default()
                    },
                    topology: TopologyCapabilities {
                        supported: vec![TopologyKind::Relay],
                        ..Default::default()
                    },
                    isolation: IsolationCapabilities {
                        shared: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
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
            credentials: vec![],
        },
        observations: Default::default(),
        route: RouteId(1),
        route_generation: 0,
        now,
        egress_observations: vec![],
    }
}

struct FailingVerifier;

impl EgressVerifier for FailingVerifier {
    fn verify(&self, _subject: VerificationSubject) -> VerificationFuture {
        Box::pin(async { VerificationOutcome::Failed(VerificationFailure::Contradicted) })
    }
}

#[tokio::test]
async fn require_before_use_reuse_after_failed_verification_fails_closed() {
    let now = Instant::now();
    let runtime = EgressRuntime::with_verifier(
        FakeTransport::new([ScriptedStep::Succeed]),
        TestClock(now),
        Arc::new(FailingVerifier),
    );

    assert_eq!(
        runtime.acquire(context(now)).await,
        Err(RuntimeError::VerificationFailed)
    );
    assert_eq!(
        runtime.acquire(context(now)).await,
        Err(RuntimeError::VerificationUnavailable)
    );
}

#[derive(Clone)]
struct BlockingVerifier {
    waiting: Arc<AtomicUsize>,
    gate: Arc<Semaphore>,
}

impl BlockingVerifier {
    fn new() -> Self {
        Self {
            waiting: Arc::new(AtomicUsize::new(0)),
            gate: Arc::new(Semaphore::new(0)),
        }
    }

    async fn wait_until_verifying(&self) {
        for _ in 0..1_000 {
            if self.waiting.load(Ordering::SeqCst) == 1 {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("verifier did not begin verification");
    }

    fn release(&self) {
        self.gate.add_permits(1);
    }
}

impl EgressVerifier for BlockingVerifier {
    fn verify(&self, subject: VerificationSubject) -> VerificationFuture {
        let verifier = self.clone();
        Box::pin(async move {
            verifier.waiting.fetch_add(1, Ordering::SeqCst);
            let permit = verifier.gate.acquire().await.unwrap();
            permit.forget();
            let evidence = subject
                .observations
                .iter()
                .filter(|observation| {
                    matches!(
                        observation.outcome,
                        observation_core::Outcome::EgressObserved(_)
                    )
                })
                .map(|observation| observation.id)
                .collect();
            VerificationOutcome::Verified(VerifiedEgress {
                id: subject.id,
                key: subject.plan.key(),
                verified_at: subject.now,
                properties: VerifiedProperties::default(),
                evidence,
            })
        })
    }
}

fn context_with_evidence(now: Instant) -> AcquireContext {
    let mut input = context(now);
    input.egress_observations = vec![EgressObservation {
        exit_address: Some("203.0.113.7".parse().unwrap()),
        ..Default::default()
    }];
    input
}

#[tokio::test]
async fn require_before_use_reuse_waits_for_pending_verification() {
    let now = Instant::now();
    let verifier = BlockingVerifier::new();
    let runtime = EgressRuntime::with_verifier(
        FakeTransport::new([ScriptedStep::Succeed]),
        TestClock(now),
        Arc::new(verifier.clone()),
    );
    let first_runtime = runtime.clone();
    let first =
        tokio::spawn(async move { first_runtime.acquire(context_with_evidence(now)).await });
    verifier.wait_until_verifying().await;

    let second_runtime = runtime.clone();
    let second =
        tokio::spawn(async move { second_runtime.acquire(context_with_evidence(now)).await });
    tokio::task::yield_now().await;
    assert!(!second.is_finished());

    verifier.release();
    assert!(first.await.unwrap().unwrap().verification.is_some());
    assert!(second.await.unwrap().unwrap().verification.is_some());
}

#[tokio::test]
async fn observe_only_reuse_without_verification_remains_allowed() {
    let now = Instant::now();
    let runtime = EgressRuntime::with_verifier(
        FakeTransport::new([ScriptedStep::Succeed]),
        TestClock(now),
        Arc::new(FailingVerifier),
    );
    let mut input = context(now);
    input.policy.verification = VerificationPolicy::ObserveOnly;

    assert!(
        runtime
            .acquire(input.clone())
            .await
            .unwrap()
            .verification
            .is_none()
    );
    assert!(runtime.acquire(input).await.unwrap().verification.is_none());
}
