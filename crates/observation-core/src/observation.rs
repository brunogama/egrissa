use egress_core::{
    ids::{FeedbackKey, ObservationId, ProviderId, UpstreamBindingId},
    request::{CountryCode, DestinationKey, Transport},
};
use std::{net::IpAddr, time::Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationScope {
    Transport,
    Endpoint(egress_core::ids::EndpointId),
    Operator(egress_core::ids::OperatorId),
    Provider(ProviderId),
    Upstream(UpstreamBindingId),
    Destination(DestinationKey),
    Route(egress_core::ids::RouteId),
    Identity(egress_core::ids::NetworkIdentityId),
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
    UnsupportedTopology,
    VerificationPassed(egress_core::ids::VerificationId),
    VerificationFailed(egress_core::ids::VerificationId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub id: ObservationId,
    pub key: FeedbackKey,
    pub observed_at: Instant,
    pub scope: ObservationScope,
    pub outcome: Outcome,
}
