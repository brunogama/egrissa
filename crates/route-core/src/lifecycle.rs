use crate::{
    planner::{
        CandidateChooser, DeterministicPlanner, PlanContext, RoutePlanner, identity_compatible,
        required_isolation, validate_context,
    },
    route::{RouteLease, RoutePlan},
};
use egress_core::{
    error::{AcquireError, LifecycleError, PlanningError, RecoveryError},
    evidence::Observation,
    ids::*,
    policy::{EgressPolicy, RecoveryPolicy},
    request::{EgressRequest, IsolationRequirement},
};
use identity_core::{
    continuity::IdentityContinuity,
    identity::{IdentityScope, NetworkIdentity},
};
use std::{collections::BTreeMap, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteState {
    Unallocated,
    Planned,
    Establishing,
    Ready,
    Failed,
    Expired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishmentResult {
    Established,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireDecision {
    Establish(RoutePlan),
    Reuse(RouteLease),
    Pending {
        route: RouteId,
        route_generation: u64,
    },
    Reject(AcquireError),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplacementDecision {
    PreserveIdentity(RoutePlan),
    ReplaceIdentity {
        old: NetworkIdentityId,
        new: NetworkIdentity,
        route: RoutePlan,
        continuity: IdentityContinuity,
    },
    Reject(RecoveryError),
}
struct Allocation {
    request: EgressRequest,
    policy: EgressPolicy,
    plan: RoutePlan,
    state: RouteState,
    retries_used: u8,
}
impl Allocation {
    fn expire(&mut self, now: Instant) {
        if now >= self.plan.expires_at()
            && matches!(
                self.state,
                RouteState::Planned | RouteState::Establishing | RouteState::Ready
            )
        {
            self.state = RouteState::Expired;
        }
    }
}
/// Synchronous control-plane owner. Serialize access with &mut or a host mutex.
/// Establishment happens only after acquire returns and the host releases its lock.
#[derive(Default)]
pub struct RouteManager {
    planner: DeterministicPlanner,
    routes: BTreeMap<RouteId, Allocation>,
    observations: Vec<Observation>,
}
impl RouteManager {
    pub fn new(planner: DeterministicPlanner) -> Self {
        Self {
            planner,
            ..Self::default()
        }
    }
    pub fn state(&self, route: RouteId) -> Option<RouteState> {
        self.routes.get(&route).map(|route| route.state)
    }
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
    /// Append immutable evidence, including stale history, without changing health or lifecycle.
    pub fn record_observation(&mut self, observation: Observation) -> Result<(), LifecycleError> {
        let current = self
            .routes
            .get(&observation.key.route)
            .is_some_and(|allocation| allocation.plan.key() == observation.key);
        self.observations.push(observation);
        if current {
            Ok(())
        } else {
            Err(LifecycleError::StaleFeedback)
        }
    }
    pub fn acquire(
        &mut self,
        context: &PlanContext<'_>,
        chooser: &mut impl CandidateChooser,
    ) -> AcquireDecision {
        if let Err(error) = validate_context(context) {
            return AcquireDecision::Reject(error.into());
        }
        if !identity_compatible(context) {
            return AcquireDecision::Reject(PlanningError::IncompatibleIdentity.into());
        }
        for allocation in self.routes.values_mut() {
            allocation.expire(context.now);
            let pinned = matches!(
                required_isolation(context),
                IsolationRequirement::PerSession | IsolationRequirement::Dedicated
            ) || matches!(
                context.identity.scope(),
                IdentityScope::Session | IdentityScope::Dedicated
            );
            if pinned
                && allocation.plan.identity() == context.identity.id()
                && allocation.request.id != context.request.id
            {
                return AcquireDecision::Reject(AcquireError::GenerationConflict);
            }
            if allocation.request.compatible_with(context.request)
                && allocation.policy == *context.policy
                && (!pinned || allocation.request.id == context.request.id)
            {
                if allocation.plan.identity() != context.identity.id()
                    || allocation.plan.identity_generation() != context.identity.generation()
                {
                    return AcquireDecision::Reject(AcquireError::GenerationConflict);
                }
                if allocation.plan.is_valid(context.now)
                    && self.planner.still_eligible(context, &allocation.plan)
                {
                    match allocation.state {
                        RouteState::Establishing => {
                            return AcquireDecision::Pending {
                                route: allocation.plan.id(),
                                route_generation: allocation.plan.route_generation(),
                            };
                        }
                        RouteState::Ready => {
                            return AcquireDecision::Reuse(RouteLease {
                                plan: allocation.plan.clone(),
                            });
                        }
                        _ => {}
                    }
                }
                return AcquireDecision::Reject(LifecycleError::InvalidTransition.into());
            }
        }
        if self.routes.contains_key(&context.route) {
            return AcquireDecision::Reject(AcquireError::GenerationConflict);
        }
        match self.planner.plan(context, chooser) {
            Ok(plan) => {
                self.routes.insert(
                    plan.id(),
                    Allocation {
                        request: context.request.clone(),
                        policy: context.policy.clone(),
                        plan: plan.clone(),
                        state: RouteState::Establishing,
                        retries_used: 0,
                    },
                );
                AcquireDecision::Establish(plan)
            }
            Err(error) => AcquireDecision::Reject(error.into()),
        }
    }
    pub fn complete(
        &mut self,
        key: FeedbackKey,
        result: EstablishmentResult,
        now: Instant,
    ) -> Result<(), LifecycleError> {
        let allocation = self
            .routes
            .get_mut(&key.route)
            .ok_or(LifecycleError::StaleFeedback)?;
        if allocation.plan.key() != key {
            return Err(LifecycleError::StaleFeedback);
        }
        if now < allocation.plan.created_at() {
            return Err(LifecycleError::InvalidValidity);
        }
        allocation.expire(now);
        if !allocation.plan.is_valid(now) {
            return Err(LifecycleError::Expired);
        }
        if allocation.state != RouteState::Establishing {
            return Err(LifecycleError::InvalidTransition);
        }
        allocation.state = match result {
            EstablishmentResult::Established => RouteState::Ready,
            EstablishmentResult::Failed => RouteState::Failed,
        };
        Ok(())
    }
    /// Continuity is an explicit assessment supplied by the host, never inferred from a status code.
    /// M0 has no verifier; a production host must obtain this assessment from verification.
    pub fn recover(
        &mut self,
        context: &PlanContext<'_>,
        continuity: IdentityContinuity,
        chooser: &mut impl CandidateChooser,
    ) -> ReplacementDecision {
        match self.prepare_replacement(context, continuity, chooser) {
            Err(error) => ReplacementDecision::Reject(error),
            Ok((plan, retries_used, old_identity)) => {
                let changed = old_identity != context.identity.id();
                self.routes.insert(
                    plan.id(),
                    Allocation {
                        request: context.request.clone(),
                        policy: context.policy.clone(),
                        plan: plan.clone(),
                        state: RouteState::Establishing,
                        retries_used,
                    },
                );
                if changed {
                    ReplacementDecision::ReplaceIdentity {
                        old: old_identity,
                        new: context.identity.clone(),
                        route: plan,
                        continuity,
                    }
                } else {
                    ReplacementDecision::PreserveIdentity(plan)
                }
            }
        }
    }
    fn prepare_replacement(
        &self,
        context: &PlanContext<'_>,
        continuity: IdentityContinuity,
        chooser: &mut impl CandidateChooser,
    ) -> Result<(RoutePlan, u8, NetworkIdentityId), RecoveryError> {
        let old = self
            .routes
            .get(&context.route)
            .ok_or(LifecycleError::StaleFeedback)?;
        if old.policy != *context.policy
            || !old.request.compatible_with(context.request)
            || context.now < old.plan.created_at()
            || !matches!(
                old.state,
                RouteState::Ready | RouteState::Failed | RouteState::Expired
            )
        {
            return Err(LifecycleError::InvalidTransition.into());
        }
        let next = old
            .plan
            .route_generation()
            .checked_add(1)
            .ok_or(LifecycleError::GenerationExhausted)?;
        if context.route_generation != next {
            return Err(LifecycleError::StaleFeedback.into());
        }
        let changed = old.plan.identity() != context.identity.id();
        let mut retries_used = old.retries_used;
        match old.policy.recovery {
            RecoveryPolicy::FailClosed => return Err(RecoveryError::PolicyDenied),
            RecoveryPolicy::RetrySameRoute { attempts } => {
                if changed || retries_used >= attempts || old.state != RouteState::Failed {
                    return Err(RecoveryError::PolicyDenied);
                }
                retries_used += 1;
            }
            RecoveryPolicy::ReplanPreservingIdentity if changed => {
                return Err(RecoveryError::PolicyDenied);
            }
            _ => {}
        }
        if changed {
            if continuity == IdentityContinuity::Preserved {
                return Err(RecoveryError::InvalidContinuity);
            }
            let next_identity = old
                .plan
                .identity_generation()
                .checked_add(1)
                .ok_or(LifecycleError::GenerationExhausted)?;
            if context.identity.generation() != next_identity {
                return Err(LifecycleError::StaleFeedback.into());
            }
        } else {
            if context.identity.generation() != old.plan.identity_generation() {
                return Err(LifecycleError::StaleFeedback.into());
            }
            if continuity != IdentityContinuity::Preserved {
                return Err(RecoveryError::ContinuityUnproven);
            }
        }
        let plan = if matches!(old.policy.recovery, RecoveryPolicy::RetrySameRoute { .. }) {
            let mut inventory = context.inventory.clone();
            inventory
                .candidates
                .retain(|candidate| candidate.topology == *old.plan.topology());
            let retry_context = PlanContext {
                inventory: &inventory,
                ..*context
            };
            self.planner.plan(&retry_context, chooser)?
        } else {
            self.planner.plan(context, chooser)?
        };
        Ok((plan, retries_used, old.plan.identity()))
    }
}
