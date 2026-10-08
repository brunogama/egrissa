use std::time::Duration;

use crate::request::IsolationRequirement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecoveryPolicy {
    #[default]
    FailClosed,
    RetrySameRoute {
        attempts: u8,
    },
    ReplanPreservingIdentity,
    ReplanAllowingIdentityChange,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeakagePolicy {
    #[default]
    ProhibitDirect,
    AllowDirect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdentityPolicy {
    pub require_observable_continuity: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutingPolicy {
    pub maximum_lifetime: Duration,
    pub distinct_operators: bool,
    pub separate_client_and_destination: bool,
}
impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            maximum_lifetime: Duration::from_secs(300),
            distinct_operators: false,
            separate_client_and_destination: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsolationPolicy {
    pub minimum: IsolationRequirement,
}
impl Default for IsolationPolicy {
    fn default() -> Self {
        Self {
            minimum: IsolationRequirement::Shared,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerificationPolicy {
    #[default]
    ObserveOnly,
    RequireBeforeUse,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EgressPolicy {
    pub identity: IdentityPolicy,
    pub routing: RoutingPolicy,
    pub isolation: IsolationPolicy,
    pub recovery: RecoveryPolicy,
    pub verification: VerificationPolicy,
    pub leakage: LeakagePolicy,
}
