//! Transport-neutral asynchronous route establishment and close contracts.

use egress_core::{ids::FeedbackKey, request::Destination};
use route_core::route::RoutePlan;
use std::{future::Future, pin::Pin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstablishmentRequest {
    pub plan: RoutePlan,
    pub destination: Destination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFailure {
    ConnectTimeout,
    ConnectionReset,
    AuthenticationRejected,
    UnsupportedTopology,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportOutcome {
    Established,
    Failed(TransportFailure),
}

pub type EstablishmentFuture = Pin<Box<dyn Future<Output = TransportOutcome> + Send + 'static>>;
pub type CloseFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub trait Transport: Send + Sync + 'static {
    fn establish(&self, request: EstablishmentRequest) -> EstablishmentFuture;
    fn close(&self, key: FeedbackKey) -> CloseFuture;
}
