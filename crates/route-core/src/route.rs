use crate::topology::RouteTopology;
use egress_core::{ids::*, request::Transport};
use std::time::Instant;

/// Immutable desired state. Only the planner can construct a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    pub(crate) key: FeedbackKey,
    pub(crate) request: RequestId,
    pub(crate) topology: RouteTopology,
    pub(crate) transport: Transport,
    pub(crate) created_at: Instant,
    pub(crate) expires_at: Instant,
    pub(crate) credentials: Vec<CredentialRef>,
}
impl RoutePlan {
    pub fn key(&self) -> FeedbackKey {
        self.key
    }
    pub fn id(&self) -> RouteId {
        self.key.route
    }
    pub fn request(&self) -> RequestId {
        self.request
    }
    pub fn identity(&self) -> NetworkIdentityId {
        self.key.identity
    }
    pub fn route_generation(&self) -> u64 {
        self.key.route_generation
    }
    pub fn identity_generation(&self) -> u64 {
        self.key.identity_generation
    }
    pub fn topology(&self) -> &RouteTopology {
        &self.topology
    }
    pub fn transport(&self) -> Transport {
        self.transport
    }
    pub fn created_at(&self) -> Instant {
        self.created_at
    }
    pub fn expires_at(&self) -> Instant {
        self.expires_at
    }
    pub fn credentials(&self) -> &[CredentialRef] {
        &self.credentials
    }
    pub fn is_valid(&self, now: Instant) -> bool {
        now >= self.created_at && now < self.expires_at
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteLease {
    pub plan: RoutePlan,
}
