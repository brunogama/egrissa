//! Transport-neutral asynchronous route establishment and close contracts.

use egress_core::{ids::FeedbackKey, request::Destination};
use route_core::route::RoutePlan;
use std::{future::Future, io, pin::Pin};

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
    EstablishedThenFailed(TransportFailure),
}

pub type EstablishmentFuture = Pin<Box<dyn Future<Output = TransportOutcome> + Send + 'static>>;
pub type CloseFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
pub type TunnelReadFuture<'a> = Pin<Box<dyn Future<Output = io::Result<Vec<u8>>> + Send + 'a>>;
pub type TunnelWriteFuture<'a> = Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>>;
pub type TunnelCloseFuture = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'static>>;

/// An adapter-owned, established data-plane tunnel.
pub trait Tunnel: Send + 'static {
    fn read(&mut self, maximum: usize) -> TunnelReadFuture<'_>;
    fn write(&mut self, bytes: Vec<u8>) -> TunnelWriteFuture<'_>;
    fn close(self: Box<Self>) -> TunnelCloseFuture;
}

pub type TunnelEstablishmentFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn Tunnel>, TransportFailure>> + Send + 'static>>;

pub trait Transport: Send + Sync + 'static {
    fn establish(&self, request: EstablishmentRequest) -> EstablishmentFuture;
    fn close(&self, key: FeedbackKey) -> CloseFuture;
}

/// A transport that yields an adapter-owned tunnel after establishment.
pub trait TunnelTransport: Send + Sync + 'static {
    fn establish_tunnel(&self, request: EstablishmentRequest) -> TunnelEstablishmentFuture;
}

/// An adapter-owned connection that preserves datagram boundaries.
pub trait DatagramTunnel: Send + 'static {
    fn send(&mut self, datagram: Vec<u8>) -> TunnelWriteFuture<'_>;
    fn receive(&mut self, maximum: usize) -> TunnelReadFuture<'_>;
    fn close(self: Box<Self>) -> TunnelCloseFuture;
}

/// An adapter-owned connection that preserves IP packet boundaries.
pub trait PacketTunnel: Send + 'static {
    fn send(&mut self, packet: Vec<u8>) -> TunnelWriteFuture<'_>;
    fn receive(&mut self, maximum: usize) -> TunnelReadFuture<'_>;
    fn close(self: Box<Self>) -> TunnelCloseFuture;
}

/// A data plane selected by the transport requirement in an established route plan.
pub enum DataPlaneTunnel {
    Datagram(Box<dyn DatagramTunnel>),
    Packet(Box<dyn PacketTunnel>),
}

/// A successful data-plane establishment and the adapter-specific protocol selection.
pub struct DataPlaneEstablishment<N> {
    pub tunnel: DataPlaneTunnel,
    pub negotiation: N,
}

pub type DataPlaneEstablishmentFuture<N> = Pin<
    Box<dyn Future<Output = Result<DataPlaneEstablishment<N>, TransportFailure>> + Send + 'static>,
>;

/// A transport that yields a datagram or packet data plane after establishment.
pub trait DataPlaneTransport: Send + Sync + 'static {
    type Negotiation: Send + 'static;

    fn establish_data_plane(
        &self,
        request: EstablishmentRequest,
    ) -> DataPlaneEstablishmentFuture<Self::Negotiation>;
}
