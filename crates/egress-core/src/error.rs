use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanningError {
    #[error("no eligible candidate")]
    NoEligibleCandidate,
    #[error("identity is incompatible, unavailable, or expired")]
    IncompatibleIdentity,
    #[error("chooser returned {index} for {count} equivalent candidates")]
    InvalidChoice { index: usize, count: usize },
    #[error("requested lifetime cannot be represented")]
    TimeOverflow,
    #[error("inventory contains duplicate identifiers")]
    InvalidInventory,
    #[error("verification before use is not implemented in M0")]
    VerificationUnavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LifecycleError {
    #[error("feedback does not address the current generation")]
    StaleFeedback,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("the object has expired")]
    Expired,
    #[error("generation exhausted")]
    GenerationExhausted,
    #[error("invalid validity interval")]
    InvalidValidity,
}
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecoveryError {
    #[error("recovery policy forbids this replacement")]
    PolicyDenied,
    #[error("identity replacement cannot claim preserved continuity")]
    InvalidContinuity,
    #[error("preserving identity requires evidence of continuity")]
    ContinuityUnproven,
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error(transparent)]
    Planning(#[from] PlanningError),
}
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AcquireError {
    #[error(transparent)]
    Planning(#[from] PlanningError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error("route ID or generation is already allocated")]
    GenerationConflict,
}
