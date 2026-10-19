use egress_core::{
    ids::{CredentialRef, EndpointId},
    request::{DestinationKey, Transport},
};
use route_core::topology::RouteTopology;
use std::{collections::BTreeSet, io, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use transport_core::{
    EstablishmentRequest, TransportFailure, Tunnel, TunnelCloseFuture, TunnelEstablishmentFuture,
    TunnelReadFuture, TunnelTransport, TunnelWriteFuture,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelayTarget(SocketAddr);

impl RelayTarget {
    pub fn new(address: SocketAddr) -> Self {
        Self(address)
    }

    fn address(self) -> SocketAddr {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayForwardingPolicy {
    allowed: BTreeSet<RelayTarget>,
}

impl RelayForwardingPolicy {
    pub fn allow_only(targets: impl IntoIterator<Item = RelayTarget>) -> Self {
        Self {
            allowed: targets.into_iter().collect(),
        }
    }

    pub fn allows(&self, target: RelayTarget) -> bool {
        self.allowed.contains(&target)
    }
}

pub struct RelayServer {
    forwarding: RelayForwardingPolicy,
}

impl RelayServer {
    pub fn new(forwarding: RelayForwardingPolicy) -> Self {
        Self { forwarding }
    }

    pub async fn serve(&self, mut inbound: TcpStream) -> io::Result<()> {
        let target = match read_target(&mut inbound).await {
            Some(target) => target,
            None => return write_response(&mut inbound, 400).await,
        };
        if !self.forwarding.allows(target) {
            return write_response(&mut inbound, 403).await;
        }
        let mut outbound = match TcpStream::connect(target.address()).await {
            Ok(stream) => stream,
            Err(_) => return write_response(&mut inbound, 502).await,
        };
        write_response(&mut inbound, 200).await?;
        tokio::io::copy_bidirectional(&mut inbound, &mut outbound)
            .await
            .map(|_| ())
    }
}

pub trait RelayAddressResolver: Send + Sync + 'static {
    fn endpoint_address(&self, endpoint: EndpointId) -> Option<SocketAddr>;
    fn destination_address(&self, destination: DestinationKey, port: u16) -> Option<RelayTarget>;
}

pub trait RelayCredentialResolver: Send + Sync + 'static {
    fn resolve(&self, reference: CredentialRef) -> Option<RelayCredentials>;
}

#[derive(Clone)]
pub struct RelayCredentials {
    token: Vec<u8>,
}

impl RelayCredentials {
    pub fn new(token: impl AsRef<[u8]>) -> Self {
        Self {
            token: token.as_ref().to_vec(),
        }
    }

    fn valid(&self) -> bool {
        !self.token.is_empty()
            && self
                .token
                .iter()
                .all(|byte| byte.is_ascii_graphic() && *byte != b' ')
    }
}

pub struct RelayClient<A, C> {
    addresses: Arc<A>,
    credentials: Arc<C>,
}

impl<A, C> Clone for RelayClient<A, C> {
    fn clone(&self) -> Self {
        Self {
            addresses: Arc::clone(&self.addresses),
            credentials: Arc::clone(&self.credentials),
        }
    }
}

impl<A, C> RelayClient<A, C> {
    pub fn new(addresses: A, credentials: C) -> Self {
        Self {
            addresses: Arc::new(addresses),
            credentials: Arc::new(credentials),
        }
    }
}

impl<A, C> RelayClient<A, C>
where
    A: RelayAddressResolver,
    C: RelayCredentialResolver,
{
    async fn open(
        &self,
        request: EstablishmentRequest,
    ) -> Result<Box<dyn Tunnel>, TransportFailure> {
        if request.plan.transport() != Transport::Tcp {
            return Err(TransportFailure::UnsupportedTopology);
        }
        let credentials = request
            .plan
            .credentials()
            .first()
            .map(|reference| {
                self.credentials
                    .resolve(*reference)
                    .filter(RelayCredentials::valid)
                    .ok_or(TransportFailure::AuthenticationRejected)
            })
            .transpose()?;
        let destination = self
            .addresses
            .destination_address(request.destination.key, request.destination.port)
            .ok_or(TransportFailure::ConnectionReset)?;
        let mut stream = match request.plan.topology() {
            RouteTopology::Relay { relay } => {
                let mut stream = self.connect(*relay).await?;
                connect(&mut stream, destination, credentials.as_ref()).await?;
                stream
            }
            RouteTopology::RelayThenSocks5 { relay, gateway, .. } => {
                let mut stream = self.connect(*relay).await?;
                connect(&mut stream, self.target(*gateway)?, credentials.as_ref()).await?;
                socks5_connect(&mut stream, destination).await?;
                stream
            }
            RouteTopology::TwoRelaysThenSocks5 {
                ingress,
                egress,
                gateway,
                ..
            } => {
                let mut stream = self.connect(*ingress).await?;
                connect(&mut stream, self.target(*egress)?, credentials.as_ref()).await?;
                connect(&mut stream, self.target(*gateway)?, credentials.as_ref()).await?;
                socks5_connect(&mut stream, destination).await?;
                stream
            }
            _ => return Err(TransportFailure::UnsupportedTopology),
        };
        stream.flush().await.map_err(map_io)?;
        Ok(Box::new(RelayTunnel { stream }))
    }

    async fn connect(&self, endpoint: EndpointId) -> Result<TcpStream, TransportFailure> {
        TcpStream::connect(self.target(endpoint)?.address())
            .await
            .map_err(map_io)
    }

    fn target(&self, endpoint: EndpointId) -> Result<RelayTarget, TransportFailure> {
        self.addresses
            .endpoint_address(endpoint)
            .map(RelayTarget::new)
            .ok_or(TransportFailure::ConnectionReset)
    }
}

impl<A, C> TunnelTransport for RelayClient<A, C>
where
    A: RelayAddressResolver,
    C: RelayCredentialResolver,
{
    fn establish_tunnel(&self, request: EstablishmentRequest) -> TunnelEstablishmentFuture {
        let client = self.clone();
        Box::pin(async move { client.open(request).await })
    }
}

struct RelayTunnel {
    stream: TcpStream,
}

impl Tunnel for RelayTunnel {
    fn read(&mut self, maximum: usize) -> TunnelReadFuture<'_> {
        Box::pin(async move {
            let mut bytes = vec![0; maximum];
            let read = self.stream.read(&mut bytes).await?;
            bytes.truncate(read);
            Ok(bytes)
        })
    }

    fn write(&mut self, bytes: Vec<u8>) -> TunnelWriteFuture<'_> {
        Box::pin(async move {
            self.stream.write_all(&bytes).await?;
            Ok(bytes.len())
        })
    }

    fn close(mut self: Box<Self>) -> TunnelCloseFuture {
        Box::pin(async move { self.stream.shutdown().await })
    }
}

async fn connect(
    stream: &mut TcpStream,
    target: RelayTarget,
    credentials: Option<&RelayCredentials>,
) -> Result<(), TransportFailure> {
    let authority = target.address();
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n").into_bytes();
    if let Some(credentials) = credentials {
        request.extend_from_slice(b"Proxy-Authorization: Bearer ");
        request.extend_from_slice(&credentials.token);
        request.extend_from_slice(b"\r\n");
    }
    request.extend_from_slice(b"\r\n");
    stream.write_all(&request).await.map_err(map_io)?;
    match read_status(stream).await? {
        200..=299 => Ok(()),
        407 => Err(TransportFailure::AuthenticationRejected),
        _ => Err(TransportFailure::ConnectionReset),
    }
}

async fn socks5_connect(
    stream: &mut TcpStream,
    target: RelayTarget,
) -> Result<(), TransportFailure> {
    stream.write_all(&[5, 1, 0]).await.map_err(map_io)?;
    if read_byte(stream).await? != 5 || read_byte(stream).await? != 0 {
        return Err(TransportFailure::AuthenticationRejected);
    }
    let SocketAddr::V4(address) = target.address() else {
        return Err(TransportFailure::UnsupportedTopology);
    };
    let mut request = vec![5, 1, 0, 1];
    request.extend_from_slice(&address.ip().octets());
    request.extend_from_slice(&address.port().to_be_bytes());
    stream.write_all(&request).await.map_err(map_io)?;
    if read_byte(stream).await? != 5
        || read_byte(stream).await? != 0
        || read_byte(stream).await? != 0
    {
        return Err(TransportFailure::ConnectionReset);
    }
    match read_byte(stream).await? {
        1 => discard(stream, 6).await,
        4 => discard(stream, 18).await,
        3 => {
            let length = usize::from(read_byte(stream).await?);
            discard(stream, length + 2).await
        }
        _ => Err(TransportFailure::ConnectionReset),
    }
}

async fn read_target(stream: &mut TcpStream) -> Option<RelayTarget> {
    let mut headers = Vec::new();
    let mut byte = [0];
    while headers.len() < 8192 {
        stream.read_exact(&mut byte).await.ok()?;
        headers.push(byte[0]);
        if headers.ends_with(b"\r\n\r\n") {
            let request = std::str::from_utf8(&headers).ok()?;
            let mut parts = request.split_whitespace();
            if parts.next()? != "CONNECT" {
                return None;
            }
            let authority = parts.next()?;
            return authority.parse().ok().map(RelayTarget::new);
        }
    }
    None
}

async fn read_status(stream: &mut TcpStream) -> Result<u16, TransportFailure> {
    let mut headers = Vec::new();
    let mut byte = [0];
    while headers.len() < 8192 {
        stream.read_exact(&mut byte).await.map_err(map_io)?;
        headers.push(byte[0]);
        if headers.ends_with(b"\r\n\r\n") {
            let status = std::str::from_utf8(&headers)
                .map_err(|_| TransportFailure::ConnectionReset)?
                .split_whitespace()
                .nth(1)
                .ok_or(TransportFailure::ConnectionReset)?
                .parse()
                .map_err(|_| TransportFailure::ConnectionReset)?;
            return Ok(status);
        }
    }
    Err(TransportFailure::ConnectionReset)
}

async fn read_byte(stream: &mut TcpStream) -> Result<u8, TransportFailure> {
    let mut byte = [0];
    stream.read_exact(&mut byte).await.map_err(map_io)?;
    Ok(byte[0])
}

async fn discard(stream: &mut TcpStream, length: usize) -> Result<(), TransportFailure> {
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await.map_err(map_io)?;
    Ok(())
}

async fn write_response(stream: &mut TcpStream, status: u16) -> io::Result<()> {
    stream
        .write_all(format!("HTTP/1.1 {status}\r\n\r\n").as_bytes())
        .await
}

fn map_io(error: io::Error) -> TransportFailure {
    if error.kind() == io::ErrorKind::TimedOut {
        TransportFailure::ConnectTimeout
    } else {
        TransportFailure::ConnectionReset
    }
}
