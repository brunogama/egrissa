//! Passive contracts only. M0 provides neither attribution nor verification engines.
use crate::{
    ids::*,
    request::{CountryCode, DestinationKey, Transport},
};
use std::{net::IpAddr, time::Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationScope {
    Transport,
    Endpoint(EndpointId),
    Operator(OperatorId),
    Provider(ProviderId),
    Upstream(UpstreamBindingId),
    Destination(DestinationKey),
    Route(RouteId),
    Identity(NetworkIdentityId),
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EgressObservation {
    pub exit_address: Option<IpAddr>,
    pub asn: Option<u32>,
    pub country: Option<CountryCode>,
    pub transport: Option<Transport>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    EstablishmentStarted,
    Established,
    ConnectTimeout,
    ConnectionReset,
    AuthenticationRejected,
    HttpStatus(u16),
    EgressObserved(EgressObservation),
    VerificationPassed(VerificationId),
    VerificationFailed(VerificationId),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub id: ObservationId,
    pub key: FeedbackKey,
    pub observed_at: Instant,
    pub scope: ObservationScope,
    pub outcome: Outcome,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    TransportFailure,
    EndpointUnavailable,
    ProviderUnavailable,
    UpstreamAuthentication,
    DestinationRejected,
    RateLimitSuspected,
    IdentityReputationSuspected,
    VerificationFailure,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Confirmed,
    Strong,
    Weak,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureAttribution {
    pub category: FailureCategory,
    pub confidence: Confidence,
    pub evidence: Vec<ObservationId>,
    pub affected_scope: ObservationScope,
}
