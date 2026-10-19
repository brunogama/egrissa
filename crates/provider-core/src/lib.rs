//! Typed provider binding inputs and deterministic projections for route planning.

use egress_core::{
    capability::EgressCapabilities,
    ids::{CredentialRef, OperatorId, ProviderId, UpstreamBindingId},
};
use route_core::planner::{CredentialLease, DestinationEligibility, UpstreamBinding};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingLifecycle {
    Active { valid_until: Option<Instant> },
    Retired,
}

impl BindingLifecycle {
    fn valid_until(&self) -> Option<Instant> {
        match self {
            Self::Active { valid_until } => *valid_until,
            Self::Retired => None,
        }
    }

    fn is_active(&self, now: Instant) -> bool {
        matches!(self, Self::Active { .. }) && self.valid_until().is_none_or(|until| now < until)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderBinding {
    pub id: UpstreamBindingId,
    pub provider: ProviderId,
    pub operator: OperatorId,
    pub lifecycle: BindingLifecycle,
    pub destinations: DestinationEligibility,
    pub capabilities: EgressCapabilities,
    pub credential: Option<CredentialRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingObservationKind {
    Available,
    Unavailable,
    Cooldown { until: Instant },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingObservation {
    pub binding: UpstreamBindingId,
    pub observed_at: Instant,
    pub kind: BindingObservationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingAvailability {
    Available,
    Unavailable,
    CoolingDown { until: Instant },
}

impl BindingAvailability {
    pub fn project(
        binding: UpstreamBindingId,
        observations: &[BindingObservation],
        now: Instant,
    ) -> Self {
        let cooldown = observations
            .iter()
            .filter(|observation| observation.binding == binding && observation.observed_at <= now)
            .filter_map(|observation| match observation.kind {
                BindingObservationKind::Cooldown { until } if until > now => Some(until),
                _ => None,
            })
            .max();
        if let Some(until) = cooldown {
            return Self::CoolingDown { until };
        }

        observations
            .iter()
            .filter(|observation| observation.binding == binding && observation.observed_at <= now)
            .filter_map(|observation| match observation.kind {
                BindingObservationKind::Available => Some((observation.observed_at, false)),
                BindingObservationKind::Unavailable => Some((observation.observed_at, true)),
                BindingObservationKind::Cooldown { .. } => None,
            })
            .max()
            .map_or(Self::Available, |(_, unavailable)| {
                if unavailable {
                    Self::Unavailable
                } else {
                    Self::Available
                }
            })
    }

    pub fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderSnapshot {
    pub upstreams: Vec<UpstreamBinding>,
    pub credentials: Vec<CredentialLease>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderInventory {
    pub bindings: Vec<ProviderBinding>,
    pub credentials: Vec<CredentialLease>,
}

impl ProviderInventory {
    pub fn materialize(
        &self,
        observations: &[BindingObservation],
        now: Instant,
    ) -> ProviderSnapshot {
        let mut upstreams = self
            .bindings
            .iter()
            .filter(|binding| binding.lifecycle.is_active(now))
            .map(|binding| UpstreamBinding {
                id: binding.id,
                operator: binding.operator,
                provider: binding.provider,
                available: BindingAvailability::project(binding.id, observations, now)
                    .is_available(),
                destinations: binding.destinations.clone(),
                capabilities: binding.capabilities.clone(),
                valid_until: binding.lifecycle.valid_until(),
                credential: binding.credential,
            })
            .collect::<Vec<_>>();
        upstreams.sort_by_key(|binding| binding.id);

        let mut credentials = self.credentials.clone();
        credentials.sort_by_key(|lease| lease.reference);
        ProviderSnapshot {
            upstreams,
            credentials,
        }
    }
}
