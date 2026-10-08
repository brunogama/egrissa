//! Correlation keys, never credentials or authorization tokens.
macro_rules! id_type {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u128);
    )+};
}
id_type!(
    WorkloadId,
    RequestId,
    NetworkIdentityId,
    RouteId,
    EndpointId,
    OperatorId,
    ProviderId,
    CredentialRef,
    UpstreamBindingId,
    ObservationId,
    VerificationId,
);

/// All asynchronous completions must carry both independent generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeedbackKey {
    pub route: RouteId,
    pub route_generation: u64,
    pub identity: NetworkIdentityId,
    pub identity_generation: u64,
}
