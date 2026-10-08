use crate::lifecycle::IdentityState;
use egress_core::{
    error::LifecycleError,
    ids::{NetworkIdentityId, WorkloadId},
};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityScope {
    Shared,
    Session,
    Workload,
    Dedicated,
}

/// Logical continuity domain, intentionally containing no IP address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkIdentity {
    id: NetworkIdentityId,
    generation: u64,
    workload: WorkloadId,
    scope: IdentityScope,
    created_at: Instant,
    valid_until: Option<Instant>,
    state: IdentityState,
    replacement_accepted: bool,
}
impl NetworkIdentity {
    pub fn allocate(
        id: NetworkIdentityId,
        workload: WorkloadId,
        scope: IdentityScope,
        now: Instant,
        valid_until: Option<Instant>,
    ) -> Result<Self, LifecycleError> {
        if valid_until.is_some_and(|end| end <= now) {
            return Err(LifecycleError::InvalidValidity);
        }
        Ok(Self {
            id,
            generation: 0,
            workload,
            scope,
            created_at: now,
            valid_until,
            state: IdentityState::Allocating,
            replacement_accepted: false,
        })
    }
    pub fn id(&self) -> NetworkIdentityId {
        self.id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn workload(&self) -> WorkloadId {
        self.workload
    }
    pub fn scope(&self) -> IdentityScope {
        self.scope
    }
    pub fn created_at(&self) -> Instant {
        self.created_at
    }
    pub fn valid_until(&self) -> Option<Instant> {
        self.valid_until
    }
    pub fn state(&self) -> IdentityState {
        self.state
    }
    pub fn is_usable(&self, now: Instant) -> bool {
        now >= self.created_at
            && self.valid_until.is_none_or(|end| now < end)
            && matches!(self.state, IdentityState::Active | IdentityState::Degraded)
    }
    pub fn transition(
        &mut self,
        id: NetworkIdentityId,
        generation: u64,
        next: IdentityState,
        now: Instant,
    ) -> Result<(), LifecycleError> {
        if id != self.id || generation != self.generation {
            return Err(LifecycleError::StaleFeedback);
        }
        if now < self.created_at {
            return Err(LifecycleError::InvalidValidity);
        }
        if self.valid_until.is_some_and(|end| now >= end) {
            if !matches!(
                self.state,
                IdentityState::Invalidated | IdentityState::Expired
            ) {
                self.state = IdentityState::Expired;
            }
            return Err(LifecycleError::Expired);
        }
        let allowed = matches!(
            (self.state, next),
            (
                IdentityState::Allocating,
                IdentityState::Active | IdentityState::Invalidated
            ) | (
                IdentityState::Active,
                IdentityState::Degraded | IdentityState::Invalidated
            ) | (
                IdentityState::Degraded,
                IdentityState::Active | IdentityState::Invalidated
            )
        );
        if !allowed {
            return Err(LifecycleError::InvalidTransition);
        }
        self.state = next;
        Ok(())
    }
    pub fn replace(
        &mut self,
        policy: egress_core::policy::RecoveryPolicy,
        new_id: NetworkIdentityId,
        continuity: crate::continuity::IdentityContinuity,
        now: Instant,
        valid_until: Option<Instant>,
    ) -> Result<IdentityReplacement, egress_core::error::RecoveryError> {
        use crate::continuity::IdentityContinuity;
        use egress_core::{error::RecoveryError, policy::RecoveryPolicy};
        if policy != RecoveryPolicy::ReplanAllowingIdentityChange {
            return Err(RecoveryError::PolicyDenied);
        }
        if new_id == self.id || continuity == IdentityContinuity::Preserved {
            return Err(RecoveryError::InvalidContinuity);
        }
        if now < self.created_at
            || self.state == IdentityState::Allocating
            || self.replacement_accepted
        {
            return Err(LifecycleError::InvalidTransition.into());
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(LifecycleError::GenerationExhausted)?;
        let mut new = Self::allocate(new_id, self.workload, self.scope, now, valid_until)?;
        new.generation = generation;
        let result = IdentityReplacement {
            old: self.id,
            old_generation: self.generation,
            new,
            continuity,
        };
        self.state = IdentityState::Invalidated;
        self.replacement_accepted = true;
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityReplacement {
    pub old: NetworkIdentityId,
    pub old_generation: u64,
    pub new: NetworkIdentity,
    pub continuity: crate::continuity::IdentityContinuity,
}
