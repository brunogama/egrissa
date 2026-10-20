//! Async orchestration over synchronous domain managers and transport contracts.

pub mod fake;

use egress_core::{
    error::{AcquireError, LifecycleError},
    ids::{FeedbackKey, NetworkIdentityId, ObservationId, RouteId},
    policy::{EgressPolicy, VerificationPolicy},
    request::{Destination, EgressRequest},
};
use identity_core::identity::NetworkIdentity;
use observation_core::{Observation, ObservationScope, ObservationSnapshot, Outcome};
use route_core::{
    lifecycle::{AcquireDecision, EstablishmentResult, RouteManager, RouteState},
    planner::{DeterministicPlanner, FirstCandidate, InventorySnapshot, PlanContext},
    route::{RouteLease, RoutePlan},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::oneshot;
use transport_core::{
    CloseFuture, DataPlaneTransport, DataPlaneTunnel, EstablishmentRequest, Transport,
    TransportFailure, TransportOutcome, Tunnel, TunnelTransport,
};
use verification_core::{EgressVerifier, VerificationOutcome, VerificationSubject, VerifiedEgress};

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Instant;
}

pub struct TunnelTransportAdapter<T> {
    adapter: Arc<T>,
    tunnels: Arc<Mutex<HashMap<FeedbackKey, Box<dyn Tunnel>>>>,
}

impl<T> TunnelTransportAdapter<T> {
    pub fn new(adapter: T) -> Self {
        Self {
            adapter: Arc::new(adapter),
            tunnels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<T> Clone for TunnelTransportAdapter<T> {
    fn clone(&self) -> Self {
        Self {
            adapter: Arc::clone(&self.adapter),
            tunnels: Arc::clone(&self.tunnels),
        }
    }
}

impl<T: TunnelTransport> Transport for TunnelTransportAdapter<T> {
    fn establish(&self, request: EstablishmentRequest) -> transport_core::EstablishmentFuture {
        let key = request.plan.key();
        let adapter = Arc::clone(&self.adapter);
        let tunnels = Arc::clone(&self.tunnels);
        Box::pin(async move {
            match adapter.establish_tunnel(request).await {
                Ok(tunnel) => {
                    let displaced = tunnels
                        .lock()
                        .expect("tunnel storage mutex poisoned")
                        .insert(key, tunnel);
                    if let Some(tunnel) = displaced {
                        let _ = tunnel.close().await;
                    }
                    TransportOutcome::Established
                }
                Err(failure) => TransportOutcome::Failed(failure),
            }
        })
    }

    fn close(&self, key: FeedbackKey) -> CloseFuture {
        let tunnel = self
            .tunnels
            .lock()
            .expect("tunnel storage mutex poisoned")
            .remove(&key);
        Box::pin(async move {
            if let Some(tunnel) = tunnel {
                let _ = tunnel.close().await;
            }
        })
    }
}

pub struct DataPlaneTransportAdapter<T> {
    adapter: Arc<T>,
    tunnels: Arc<Mutex<HashMap<FeedbackKey, DataPlaneTunnel>>>,
}

impl<T> DataPlaneTransportAdapter<T> {
    pub fn new(adapter: T) -> Self {
        Self {
            adapter: Arc::new(adapter),
            tunnels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<T> Clone for DataPlaneTransportAdapter<T> {
    fn clone(&self) -> Self {
        Self {
            adapter: Arc::clone(&self.adapter),
            tunnels: Arc::clone(&self.tunnels),
        }
    }
}

impl<T: DataPlaneTransport> Transport for DataPlaneTransportAdapter<T> {
    fn establish(&self, request: EstablishmentRequest) -> transport_core::EstablishmentFuture {
        let key = request.plan.key();
        let adapter = Arc::clone(&self.adapter);
        let tunnels = Arc::clone(&self.tunnels);
        Box::pin(async move {
            match adapter.establish_data_plane(request).await {
                Ok(establishment) => {
                    let displaced = tunnels
                        .lock()
                        .expect("data-plane tunnel storage mutex poisoned")
                        .insert(key, establishment.tunnel);
                    if let Some(tunnel) = displaced {
                        match tunnel {
                            DataPlaneTunnel::Datagram(tunnel) => {
                                let _ = tunnel.close().await;
                            }
                            DataPlaneTunnel::Packet(tunnel) => {
                                let _ = tunnel.close().await;
                            }
                        }
                    }
                    TransportOutcome::Established
                }
                Err(failure) => TransportOutcome::Failed(failure),
            }
        })
    }

    fn close(&self, key: FeedbackKey) -> CloseFuture {
        let tunnel = self
            .tunnels
            .lock()
            .expect("data-plane tunnel storage mutex poisoned")
            .remove(&key);
        Box::pin(async move {
            if let Some(tunnel) = tunnel {
                match tunnel {
                    DataPlaneTunnel::Datagram(tunnel) => {
                        let _ = tunnel.close().await;
                    }
                    DataPlaneTunnel::Packet(tunnel) => {
                        let _ = tunnel.close().await;
                    }
                }
            }
        })
    }
}
#[derive(Debug, Clone)]
pub struct AcquireContext {
    pub request: EgressRequest,
    pub policy: EgressPolicy,
    pub identity: NetworkIdentity,
    pub inventory: InventorySnapshot,
    pub observations: ObservationSnapshot,
    pub route: RouteId,
    pub route_generation: u64,
    pub now: Instant,
    pub egress_observations: Vec<observation_core::EgressObservation>,
}

impl AcquireContext {
    fn plan_context(&self) -> PlanContext<'_> {
        PlanContext {
            request: &self.request,
            policy: &self.policy,
            identity: &self.identity,
            inventory: &self.inventory,
            observations: &self.observations,
            now: self.now,
            route: self.route,
            route_generation: self.route_generation,
        }
    }

    fn destination(&self) -> Destination {
        self.request.destination.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    Acquire(AcquireError),
    Establishment(TransportFailure),
    Lifecycle(LifecycleError),
    VerificationUnavailable,
    VerificationFailed,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquiredLease {
    pub lease: RouteLease,
    pub verification: Option<VerifiedEgress>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    AcquireStarted {
        request: egress_core::ids::RequestId,
    },
    AcquireRejected {
        request: egress_core::ids::RequestId,
    },
    RoutePlanned {
        key: FeedbackKey,
    },
    RoutePending {
        key: FeedbackKey,
    },
    EstablishmentStarted {
        key: FeedbackKey,
    },
    EstablishmentSucceeded {
        key: FeedbackKey,
    },
    EstablishmentFailed {
        key: FeedbackKey,
        failure: TransportFailure,
    },
    RecoveryStarted {
        key: FeedbackKey,
    },
    ReplacementPreserved {
        old: FeedbackKey,
        new: FeedbackKey,
    },
    ReplacementIdentityChanged {
        old: FeedbackKey,
        new: FeedbackKey,
    },
    ObservationRecorded {
        id: ObservationId,
        key: FeedbackKey,
    },
    LeaseIssued {
        key: FeedbackKey,
        expires_at: Instant,
    },
    VerificationCompleted {
        key: FeedbackKey,
        verified: bool,
    },
    RouteExpired {
        route: RouteId,
    },
    FailureAttributed {
        key: FeedbackKey,
        category: observation_core::FailureCategory,
        scope: observation_core::FailureScope,
        evidence: Vec<ObservationId>,
    },
    Shutdown,
}

struct PendingEstablishment {
    waiters: Vec<oneshot::Sender<Result<AcquiredLease, RuntimeError>>>,
}

struct ControlPlane {
    manager: RouteManager,
    pending: HashMap<FeedbackKey, PendingEstablishment>,
    events: Vec<RuntimeEvent>,
    next_observation: u128,
    next_verification: u128,
    verifications: HashMap<FeedbackKey, VerifiedEgress>,
    retained_transports: HashSet<FeedbackKey>,
    shutdown: bool,
}

struct RuntimeInner<T, C> {
    control: Mutex<ControlPlane>,
    transport: T,
    clock: C,
    verifier: Option<Arc<dyn EgressVerifier>>,
}

pub struct EgressRuntime<T, C> {
    inner: Arc<RuntimeInner<T, C>>,
}

fn failure_observation_outcome(failure: TransportFailure) -> Outcome {
    match failure {
        TransportFailure::ConnectTimeout => Outcome::ConnectTimeout,
        TransportFailure::ConnectionReset => Outcome::ConnectionReset,
        TransportFailure::AuthenticationRejected => Outcome::AuthenticationRejected,
        TransportFailure::UnsupportedTopology => Outcome::UnsupportedTopology,
    }
}

impl<T: Transport, C: Clock> Clone for EgressRuntime<T, C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Transport, C: Clock> EgressRuntime<T, C> {
    pub fn new(transport: T, clock: C) -> Self {
        Self::build(transport, clock, None)
    }

    pub fn with_verifier(transport: T, clock: C, verifier: Arc<dyn EgressVerifier>) -> Self {
        Self::build(transport, clock, Some(verifier))
    }

    fn build(transport: T, clock: C, verifier: Option<Arc<dyn EgressVerifier>>) -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                control: Mutex::new(ControlPlane {
                    manager: RouteManager::new(DeterministicPlanner::default()),
                    pending: HashMap::new(),
                    events: Vec::new(),
                    next_observation: 1,
                    next_verification: 1,
                    retained_transports: HashSet::new(),
                    verifications: HashMap::new(),
                    shutdown: false,
                }),
                transport,
                clock,
                verifier,
            }),
        }
    }

    pub async fn acquire(&self, context: AcquireContext) -> Result<AcquiredLease, RuntimeError> {
        let expired_keys = {
            let mut control = self
                .inner
                .control
                .lock()
                .expect("control plane mutex poisoned");
            if control.shutdown {
                return Err(RuntimeError::Shutdown);
            }
            let expired_routes = control.manager.expire_routes(context.now);
            for route in &expired_routes {
                control
                    .events
                    .push(RuntimeEvent::RouteExpired { route: *route });
            }
            let expired_keys = control
                .retained_transports
                .iter()
                .filter(|key| expired_routes.contains(&key.route))
                .copied()
                .collect::<Vec<_>>();
            for key in &expired_keys {
                control.retained_transports.remove(key);
            }
            expired_keys
        };
        self.close_transport_keys(expired_keys).await;

        let (sender, receiver) = oneshot::channel();
        let mut owner_plan = None;
        {
            let mut control = self
                .inner
                .control
                .lock()
                .expect("control plane mutex poisoned");
            if control.shutdown {
                return Err(RuntimeError::Shutdown);
            }
            if context.policy.verification == VerificationPolicy::RequireBeforeUse
                && self.inner.verifier.is_none()
            {
                return Err(RuntimeError::VerificationUnavailable);
            }
            control.events.push(RuntimeEvent::AcquireStarted {
                request: context.request.id,
            });
            let decision = control
                .manager
                .acquire(&context.plan_context(), &mut FirstCandidate);
            match decision {
                AcquireDecision::Reuse(lease) => {
                    let verification = control.verifications.get(&lease.plan.key()).cloned();
                    if verification.is_some()
                        || context.policy.verification != VerificationPolicy::RequireBeforeUse
                    {
                        return Ok(AcquiredLease {
                            lease,
                            verification,
                        });
                    }
                    let key = lease.plan.key();
                    control.events.push(RuntimeEvent::RoutePending { key });
                    let Some(pending) = control.pending.get_mut(&key) else {
                        return Err(RuntimeError::VerificationUnavailable);
                    };
                    pending.waiters.push(sender);
                }
                AcquireDecision::Reject(error) => {
                    control.events.push(RuntimeEvent::AcquireRejected {
                        request: context.request.id,
                    });
                    return Err(RuntimeError::Acquire(error));
                }
                AcquireDecision::Establish(plan) => {
                    let key = plan.key();
                    control.events.push(RuntimeEvent::RoutePlanned { key });
                    control
                        .pending
                        .entry(key)
                        .or_insert_with(|| PendingEstablishment {
                            waiters: Vec::new(),
                        })
                        .waiters
                        .push(sender);
                    owner_plan = Some(plan);
                }
                AcquireDecision::Pending {
                    route,
                    route_generation,
                } => {
                    let key = FeedbackKey {
                        route,
                        route_generation,
                        identity: context.identity.id(),
                        identity_generation: context.identity.generation(),
                    };
                    control.events.push(RuntimeEvent::RoutePending { key });
                    let Some(pending) = control.pending.get_mut(&key) else {
                        return Err(RuntimeError::Lifecycle(LifecycleError::StaleFeedback));
                    };
                    pending.waiters.push(sender);
                }
            }
        }
        if let Some(plan) = owner_plan {
            let task = self.clone();
            let context = context.clone();
            tokio::spawn(async move {
                task.establish(plan, context).await;
            });
        }
        receiver.await.unwrap_or(Err(RuntimeError::Shutdown))
    }
    async fn establish(&self, mut plan: RoutePlan, mut context: AcquireContext) {
        loop {
            let key = plan.key();
            let started = self.observation(
                key,
                &plan,
                Outcome::EstablishmentStarted,
                self.inner.clock.now(),
            );
            {
                let mut control = self
                    .inner
                    .control
                    .lock()
                    .expect("control plane mutex poisoned");
                if control.shutdown {
                    self.settle_locked(&mut control, key, Err(RuntimeError::Shutdown));
                    return;
                }
                if let Err(error) = control.manager.record_observation(started.clone()) {
                    self.settle_locked(&mut control, key, Err(RuntimeError::Lifecycle(error)));
                    return;
                }
                control
                    .events
                    .push(RuntimeEvent::EstablishmentStarted { key });
                control.events.push(RuntimeEvent::ObservationRecorded {
                    id: started.id,
                    key,
                });
            }
            let result = self
                .inner
                .transport
                .establish(EstablishmentRequest {
                    plan: plan.clone(),
                    destination: context.destination(),
                })
                .await;
            let now = self.inner.clock.now();
            let retains_transport = !matches!(result, TransportOutcome::Failed(_));
            let (outcome, terminal_failure) = match result {
                TransportOutcome::Established => (Outcome::Established, None),
                TransportOutcome::EstablishedThenFailed(failure) => {
                    (Outcome::Established, Some(failure))
                }
                TransportOutcome::Failed(failure) => {
                    (failure_observation_outcome(failure), Some(failure))
                }
            };
            let observation = self.observation(key, &plan, outcome, now);
            let failure_observation = terminal_failure.map(|failure| {
                self.observation(key, &plan, failure_observation_outcome(failure), now)
            });
            let complete = (|| {
                let mut control = self
                    .inner
                    .control
                    .lock()
                    .expect("control plane mutex poisoned");
                if retains_transport {
                    control.retained_transports.insert(key);
                }
                if control.shutdown {
                    self.settle_locked(&mut control, key, Err(RuntimeError::Shutdown));
                    return None;
                }
                if let Err(error) = control.manager.record_observation(observation.clone()) {
                    self.settle_locked(&mut control, key, Err(RuntimeError::Lifecycle(error)));
                    return None;
                }
                control.events.push(RuntimeEvent::ObservationRecorded {
                    id: observation.id,
                    key,
                });
                let state = match result {
                    TransportOutcome::Established => {
                        control
                            .events
                            .push(RuntimeEvent::EstablishmentSucceeded { key });
                        EstablishmentResult::Established
                    }
                    TransportOutcome::EstablishedThenFailed(failure) => {
                        control
                            .events
                            .push(RuntimeEvent::EstablishmentSucceeded { key });
                        let failed = failure_observation.expect("terminal failure has observation");
                        if let Err(error) = control.manager.record_observation(failed.clone()) {
                            self.settle_locked(
                                &mut control,
                                key,
                                Err(RuntimeError::Lifecycle(error)),
                            );
                            return None;
                        }
                        control
                            .events
                            .push(RuntimeEvent::ObservationRecorded { id: failed.id, key });
                        control
                            .events
                            .push(RuntimeEvent::EstablishmentFailed { key, failure });
                        EstablishmentResult::Failed
                    }
                    TransportOutcome::Failed(failure) => {
                        control
                            .events
                            .push(RuntimeEvent::EstablishmentFailed { key, failure });
                        EstablishmentResult::Failed
                    }
                };
                Some(control.manager.complete(key, state, now))
            })();
            let Some(complete) = complete else {
                self.close_retained_transport(key).await;
                return;
            };
            if let Err(error) = complete {
                {
                    let mut control = self
                        .inner
                        .control
                        .lock()
                        .expect("control plane mutex poisoned");
                    self.settle_locked(&mut control, key, Err(RuntimeError::Lifecycle(error)));
                }
                self.close_retained_transport(key).await;
                return;
            }
            match result {
                TransportOutcome::Established => {
                    if let Err(error) = self.record_egress_observations(&plan, &context) {
                        let mut control = self
                            .inner
                            .control
                            .lock()
                            .expect("control plane mutex poisoned");
                        self.settle_locked(&mut control, key, Err(error));
                        return;
                    }
                    let verification = self.verify(&plan, &context).await;
                    match verification {
                        Ok(verification) => {
                            let mut control = self
                                .inner
                                .control
                                .lock()
                                .expect("control plane mutex poisoned");
                            if control.shutdown {
                                self.settle_locked(&mut control, key, Err(RuntimeError::Shutdown));
                                return;
                            }
                            let lease = RouteLease { plan: plan.clone() };
                            control.events.push(RuntimeEvent::LeaseIssued {
                                key,
                                expires_at: plan.expires_at(),
                            });
                            self.settle_locked(
                                &mut control,
                                key,
                                Ok(AcquiredLease {
                                    lease,
                                    verification,
                                }),
                            );
                        }
                        Err(error) => {
                            {
                                let mut control = self
                                    .inner
                                    .control
                                    .lock()
                                    .expect("control plane mutex poisoned");
                                if control.shutdown {
                                    self.settle_locked(
                                        &mut control,
                                        key,
                                        Err(RuntimeError::Shutdown),
                                    );
                                    return;
                                }
                                control.manager.fail_ready(key).expect(
                                    "established route must remain ready during verification",
                                );
                                control
                                    .manager
                                    .remove_terminal(key)
                                    .expect("failed route must be removable");
                            }
                            self.close_retained_transport(key).await;
                            let mut control = self
                                .inner
                                .control
                                .lock()
                                .expect("control plane mutex poisoned");
                            self.settle_locked(&mut control, key, Err(error));
                        }
                    }
                    return;
                }
                TransportOutcome::EstablishedThenFailed(failure) => {
                    if let Err(error) = self.record_egress_observations(&plan, &context) {
                        {
                            let mut control = self
                                .inner
                                .control
                                .lock()
                                .expect("control plane mutex poisoned");
                            self.settle_locked(&mut control, key, Err(error));
                        }
                        self.close_retained_transport(key).await;
                        return;
                    }
                    self.close_retained_transport(key).await;
                    match self.recover_plan(&plan, context) {
                        Some((replacement, next_context)) => {
                            plan = replacement;
                            context = next_context;
                        }
                        None => {
                            let mut control = self
                                .inner
                                .control
                                .lock()
                                .expect("control plane mutex poisoned");
                            let _ = control.manager.remove_terminal(key);
                            self.settle_locked(
                                &mut control,
                                key,
                                Err(RuntimeError::Establishment(failure)),
                            );
                            return;
                        }
                    }
                }
                TransportOutcome::Failed(failure) => match self.recover_plan(&plan, context) {
                    Some((replacement, next_context)) => {
                        plan = replacement;
                        context = next_context;
                    }
                    None => {
                        let mut control = self
                            .inner
                            .control
                            .lock()
                            .expect("control plane mutex poisoned");
                        let _ = control.manager.remove_terminal(key);
                        self.settle_locked(
                            &mut control,
                            key,
                            Err(RuntimeError::Establishment(failure)),
                        );
                        return;
                    }
                },
            }
        }
    }

    fn recover_plan(
        &self,
        plan: &RoutePlan,
        mut context: AcquireContext,
    ) -> Option<(RoutePlan, AcquireContext)> {
        use identity_core::continuity::IdentityContinuity;
        use route_core::lifecycle::ReplacementDecision;

        let key = plan.key();
        let mut control = self
            .inner
            .control
            .lock()
            .expect("control plane mutex poisoned");
        if control.shutdown {
            return None;
        }
        let route_observations = control
            .manager
            .observations()
            .iter()
            .filter(|observation| observation.key == key)
            .cloned()
            .collect::<Vec<_>>();
        let attribution = observation_core::attribute(&route_observations);
        if let Some(attribution) = attribution {
            control.events.push(RuntimeEvent::FailureAttributed {
                key,
                category: attribution.category,
                scope: attribution.affected_scope.clone(),
                evidence: attribution.evidence.clone(),
            });
            context.observations = observation_core::HealthProjection::project(
                &[attribution],
                context.now,
                observation_core::HealthPolicy::default(),
            )
            .into_snapshot();
        }
        context.route_generation = plan.route_generation().checked_add(1)?;
        context.now = self.inner.clock.now();
        if context.policy.recovery == egress_core::policy::RecoveryPolicy::FailClosed {
            return None;
        }
        let continuity = control
            .verifications
            .get(&key)
            .map_or(IdentityContinuity::Unknown, |verified| {
                verification_core::assess_continuity(verified).continuity
            });
        if context.policy.recovery
            == egress_core::policy::RecoveryPolicy::ReplanAllowingIdentityChange
        {
            let replacement_id = NetworkIdentityId(context.identity.id().0.checked_add(1)?);
            let replacement = context
                .identity
                .replace(
                    context.policy.recovery,
                    replacement_id,
                    IdentityContinuity::Unknown,
                    context.now,
                    None,
                )
                .ok()?;
            let mut identity = replacement.new;
            identity
                .transition(
                    identity.id(),
                    identity.generation(),
                    identity_core::lifecycle::IdentityState::Active,
                    context.now,
                )
                .ok()?;
            context.identity = identity;
        }
        let decision =
            control
                .manager
                .recover(&context.plan_context(), continuity, &mut FirstCandidate);
        let new_plan = match decision {
            ReplacementDecision::PreserveIdentity(plan) => plan,
            ReplacementDecision::ReplaceIdentity { route, .. } => route,
            ReplacementDecision::Reject(_error) => {
                let _ = control.manager.remove_terminal(key);
                return None;
            }
        };
        control.events.push(RuntimeEvent::RecoveryStarted { key });
        let pending = control.pending.remove(&key)?;
        let new_key = new_plan.key();
        control.pending.insert(new_key, pending);
        if new_key.identity != key.identity {
            control
                .events
                .push(RuntimeEvent::ReplacementIdentityChanged {
                    old: key,
                    new: new_key,
                });
        } else {
            control.events.push(RuntimeEvent::ReplacementPreserved {
                old: key,
                new: new_key,
            });
        }
        Some((new_plan, context))
    }

    fn record_egress_observations(
        &self,
        plan: &RoutePlan,
        context: &AcquireContext,
    ) -> Result<(), RuntimeError> {
        let key = plan.key();
        for evidence in &context.egress_observations {
            let mut observation = self.observation(
                key,
                plan,
                Outcome::EgressObserved(evidence.clone()),
                self.inner.clock.now(),
            );
            observation.scope = ObservationScope::Transport;
            let mut control = self
                .inner
                .control
                .lock()
                .expect("control plane mutex poisoned");
            if control.shutdown {
                return Err(RuntimeError::Shutdown);
            }
            control
                .manager
                .record_observation(observation.clone())
                .map_err(RuntimeError::Lifecycle)?;
            control.events.push(RuntimeEvent::ObservationRecorded {
                id: observation.id,
                key,
            });
        }
        Ok(())
    }

    async fn verify(
        &self,
        plan: &RoutePlan,
        context: &AcquireContext,
    ) -> Result<Option<VerifiedEgress>, RuntimeError> {
        let Some(verifier) = self.inner.verifier.clone() else {
            return if context.policy.verification == VerificationPolicy::RequireBeforeUse {
                Err(RuntimeError::VerificationUnavailable)
            } else {
                Ok(None)
            };
        };
        let key = plan.key();
        let subject = {
            let mut control = self
                .inner
                .control
                .lock()
                .expect("control plane mutex poisoned");
            if control.shutdown {
                return Err(RuntimeError::Shutdown);
            }
            let id = egress_core::ids::VerificationId(control.next_verification);
            control.next_verification += 1;
            VerificationSubject {
                id,
                request: context.request.clone(),
                plan: plan.clone(),
                identity: context.identity.clone(),
                observations: control.manager.observations().to_vec(),
                now: self.inner.clock.now(),
            }
        };
        match verifier.verify(subject.clone()).await {
            VerificationOutcome::Verified(verified)
                if verified.key == key && !verified.evidence.is_empty() =>
            {
                let observation = self.observation(
                    key,
                    plan,
                    Outcome::VerificationPassed(verified.id),
                    self.inner.clock.now(),
                );
                let mut control = self
                    .inner
                    .control
                    .lock()
                    .expect("control plane mutex poisoned");
                if control.shutdown {
                    return Err(RuntimeError::Shutdown);
                }
                control
                    .manager
                    .record_observation(observation.clone())
                    .map_err(RuntimeError::Lifecycle)?;
                control.events.push(RuntimeEvent::ObservationRecorded {
                    id: observation.id,
                    key,
                });
                control.events.push(RuntimeEvent::VerificationCompleted {
                    key,
                    verified: true,
                });
                control.verifications.insert(key, verified.clone());
                Ok(Some(verified))
            }
            VerificationOutcome::Verified(_) => Err(RuntimeError::VerificationUnavailable),
            VerificationOutcome::Failed(_) => {
                let observation = self.observation(
                    key,
                    plan,
                    Outcome::VerificationFailed(subject.id),
                    self.inner.clock.now(),
                );
                let mut control = self
                    .inner
                    .control
                    .lock()
                    .expect("control plane mutex poisoned");
                if control.shutdown {
                    return Err(RuntimeError::Shutdown);
                }
                control
                    .manager
                    .record_observation(observation.clone())
                    .map_err(RuntimeError::Lifecycle)?;
                control.events.push(RuntimeEvent::ObservationRecorded {
                    id: observation.id,
                    key,
                });
                control.events.push(RuntimeEvent::VerificationCompleted {
                    key,
                    verified: false,
                });
                if context.policy.verification == VerificationPolicy::RequireBeforeUse {
                    Err(RuntimeError::VerificationFailed)
                } else {
                    Ok(None)
                }
            }
            VerificationOutcome::InsufficientEvidence => {
                if context.policy.verification == VerificationPolicy::RequireBeforeUse {
                    Err(RuntimeError::VerificationUnavailable)
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn observation(
        &self,
        key: FeedbackKey,
        plan: &RoutePlan,
        outcome: Outcome,
        observed_at: Instant,
    ) -> Observation {
        let scope = match plan.topology().upstream() {
            Some(upstream) => ObservationScope::Upstream(upstream),
            None => plan
                .topology()
                .endpoints()
                .first()
                .map_or(ObservationScope::Route(key.route), |(endpoint, _)| {
                    ObservationScope::Endpoint(*endpoint)
                }),
        };
        let mut control = self
            .inner
            .control
            .lock()
            .expect("control plane mutex poisoned");
        let id = ObservationId(control.next_observation);
        control.next_observation += 1;
        Observation {
            id,
            key,
            observed_at,
            scope,
            outcome,
        }
    }

    fn settle_locked(
        &self,
        control: &mut ControlPlane,
        key: FeedbackKey,
        result: Result<AcquiredLease, RuntimeError>,
    ) {
        if let Some(pending) = control.pending.remove(&key) {
            for waiter in pending.waiters {
                let _ = waiter.send(result.clone());
            }
        }
    }

    async fn close_transport_keys(&self, keys: Vec<FeedbackKey>) {
        for key in keys {
            self.inner.transport.close(key).await;
        }
    }

    async fn close_retained_transport(&self, key: FeedbackKey) {
        let should_close = self
            .inner
            .control
            .lock()
            .expect("control plane mutex poisoned")
            .retained_transports
            .remove(&key);
        if should_close {
            self.inner.transport.close(key).await;
        }
    }

    pub fn record_observation(&self, observation: Observation) -> Result<(), RuntimeError> {
        let mut control = self
            .inner
            .control
            .lock()
            .expect("control plane mutex poisoned");
        if control.shutdown {
            return Err(RuntimeError::Shutdown);
        }
        let key = observation.key;
        let id = observation.id;
        let result = control.manager.record_observation(observation);
        control
            .events
            .push(RuntimeEvent::ObservationRecorded { id, key });
        result.map_err(RuntimeError::Lifecycle)
    }

    pub fn complete_feedback(
        &self,
        key: FeedbackKey,
        result: EstablishmentResult,
    ) -> Result<(), RuntimeError> {
        let now = self.inner.clock.now();
        self.inner
            .control
            .lock()
            .expect("control plane mutex poisoned")
            .manager
            .complete(key, result, now)
            .map_err(RuntimeError::Lifecycle)
    }

    pub fn state(&self, route: RouteId) -> Option<RouteState> {
        self.inner
            .control
            .lock()
            .expect("control plane mutex poisoned")
            .manager
            .state(route)
    }

    pub fn events(&self) -> Vec<RuntimeEvent> {
        self.inner
            .control
            .lock()
            .expect("control plane mutex poisoned")
            .events
            .clone()
    }

    pub fn observations(&self) -> Vec<Observation> {
        self.inner
            .control
            .lock()
            .expect("control plane mutex poisoned")
            .manager
            .observations()
            .to_vec()
    }

    pub async fn shutdown(&self) {
        let (pending, retained_transports) = {
            let mut control = self
                .inner
                .control
                .lock()
                .expect("control plane mutex poisoned");
            if control.shutdown {
                return;
            }
            control.shutdown = true;
            control.events.push(RuntimeEvent::Shutdown);
            let pending = std::mem::take(&mut control.pending);
            let retained_transports: Vec<_> = std::mem::take(&mut control.retained_transports)
                .into_iter()
                .collect();
            (pending, retained_transports)
        };
        for (_, entry) in pending {
            for waiter in entry.waiters {
                let _ = waiter.send(Err(RuntimeError::Shutdown));
            }
        }
        self.close_transport_keys(retained_transports).await;
    }
}
