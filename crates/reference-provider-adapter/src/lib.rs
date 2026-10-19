//! Configurable reference adapter that publishes generic provider facts.

use provider_core::{BindingObservation, ProviderBinding, ProviderInventory};
use route_core::planner::{Candidate, CredentialLease, Endpoint, InventorySnapshot};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceProviderConfig {
    pub binding: ProviderBinding,
    pub credentials: Vec<CredentialLease>,
    pub endpoints: Vec<Endpoint>,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceProviderAdapter {
    config: ReferenceProviderConfig,
}

impl ReferenceProviderAdapter {
    pub fn new(config: ReferenceProviderConfig) -> Self {
        Self { config }
    }

    pub fn inventory(
        &self,
        observations: &[BindingObservation],
        now: Instant,
    ) -> InventorySnapshot {
        let provider = ProviderInventory {
            bindings: vec![self.config.binding.clone()],
            credentials: self.config.credentials.clone(),
        }
        .materialize(observations, now);
        InventorySnapshot {
            endpoints: self.config.endpoints.clone(),
            upstreams: provider.upstreams,
            candidates: self.config.candidates.clone(),
            credentials: provider.credentials,
        }
    }
}
