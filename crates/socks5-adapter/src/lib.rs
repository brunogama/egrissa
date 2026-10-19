//! SOCKS5 TCP CONNECT adapter. Secrets are resolved here, never in domain objects.

use egress_core::{
    ids::{CredentialRef, EndpointId},
    request::{DestinationKey, Transport},
};
use route_core::topology::RouteTopology;
use std::{io, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use transport_core::{
    EstablishmentRequest, TransportFailure, Tunnel, TunnelCloseFuture, TunnelEstablishmentFuture,
    TunnelReadFuture, TunnelTransport, TunnelWriteFuture,
};

/// Resolves opaque domain identifiers to the addresses used only by this adapter.
pub trait Socks5AddressResolver: Send + Sync + 'static {
    fn gateway_address(&self, gateway: EndpointId) -> Option<SocketAddr>;
    fn destination_address(&self, destination: DestinationKey, port: u16) -> Option<SocketAddr>;
}

/// Resolves a credential reference without exposing a secret to the domain model.
pub trait Socks5CredentialResolver: Send + Sync + 'static {
    fn resolve(&self, reference: CredentialRef) -> Option<Socks5Credentials>;
}

/// SOCKS5 username/password credentials held only by the adapter boundary.
#[derive(Clone)]
pub struct Socks5Credentials {
    username: Vec<u8>,
    password: Vec<u8>,
}

impl Socks5Credentials {
    pub fn new(username: impl AsRef<[u8]>, password: impl AsRef<[u8]>) -> Self {
        Self {
            username: username.as_ref().to_vec(),
            password: password.as_ref().to_vec(),
        }
    }

    fn is_valid(&self) -> bool {
        !self.username.is_empty()
            && self.username.len() <= u8::MAX as usize
            && !self.password.is_empty()
            && self.password.len() <= u8::MAX as usize
    }
}

pub struct Socks5Adapter<A, C> {
    addresses: Arc<A>,
    credentials: Arc<C>,
}

impl<A, C> Clone for Socks5Adapter<A, C> {
    fn clone(&self) -> Self {
        Self {
            addresses: Arc::clone(&self.addresses),
            credentials: Arc::clone(&self.credentials),
        }
    }
}

impl<A, C> Socks5Adapter<A, C> {
    pub fn new(addresses: A, credentials: C) -> Self {
        Self {
            addresses: Arc::new(addresses),
            credentials: Arc::new(credentials),
        }
    }
}

impl<A, C> Socks5Adapter<A, C>
where
    A: Socks5AddressResolver,
    C: Socks5CredentialResolver,
{
    async fn open(
        &self,
        request: EstablishmentRequest,
    ) -> Result<Box<dyn Tunnel>, TransportFailure> {
        let RouteTopology::Socks5 { gateway, .. } = request.plan.topology() else {
            return Err(TransportFailure::UnsupportedTopology);
        };
        if request.plan.transport() != Transport::Tcp {
            return Err(TransportFailure::UnsupportedTopology);
        }
        let gateway = self
            .addresses
            .gateway_address(*gateway)
            .ok_or(TransportFailure::ConnectionReset)?;
        let destination = self
            .addresses
            .destination_address(request.destination.key, request.destination.port)
            .ok_or(TransportFailure::ConnectionReset)?;
        let credentials = match request.plan.credentials().first() {
            Some(reference) => Some(
                self.credentials
                    .resolve(*reference)
                    .ok_or(TransportFailure::AuthenticationRejected)?,
            ),
            None => None,
        };
        if credentials
            .as_ref()
            .is_some_and(|credential| !credential.is_valid())
        {
            return Err(TransportFailure::AuthenticationRejected);
        }
        let mut stream = TcpStream::connect(gateway).await.map_err(map_io_error)?;
        negotiate(&mut stream, credentials.as_ref()).await?;
        connect(&mut stream, destination).await?;
        Ok(Box::new(Socks5Tunnel { stream }))
    }
}

impl<A, C> TunnelTransport for Socks5Adapter<A, C>
where
    A: Socks5AddressResolver,
    C: Socks5CredentialResolver,
{
    fn establish_tunnel(&self, request: EstablishmentRequest) -> TunnelEstablishmentFuture {
        let adapter = self.clone();
        Box::pin(async move { adapter.open(request).await })
    }
}

struct Socks5Tunnel {
    stream: TcpStream,
}

impl Tunnel for Socks5Tunnel {
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

async fn negotiate(
    stream: &mut TcpStream,
    credentials: Option<&Socks5Credentials>,
) -> Result<(), TransportFailure> {
    let methods: &[u8] = if credentials.is_some() { &[0, 2] } else { &[0] };
    let mut greeting = vec![5, methods.len() as u8];
    greeting.extend_from_slice(methods);
    stream.write_all(&greeting).await.map_err(map_io_error)?;
    let version = read_byte(stream).await?;
    let method = read_byte(stream).await?;
    if version != 5 {
        return Err(TransportFailure::ConnectionReset);
    }
    match method {
        0 => Ok(()),
        2 => {
            authenticate(
                stream,
                credentials.ok_or(TransportFailure::AuthenticationRejected)?,
            )
            .await
        }
        _ => Err(TransportFailure::AuthenticationRejected),
    }
}

async fn authenticate(
    stream: &mut TcpStream,
    credentials: &Socks5Credentials,
) -> Result<(), TransportFailure> {
    let mut request = vec![1, credentials.username.len() as u8];
    request.extend_from_slice(&credentials.username);
    request.push(credentials.password.len() as u8);
    request.extend_from_slice(&credentials.password);
    stream.write_all(&request).await.map_err(map_io_error)?;
    if read_byte(stream).await? != 1 || read_byte(stream).await? != 0 {
        return Err(TransportFailure::AuthenticationRejected);
    }
    Ok(())
}

async fn connect(stream: &mut TcpStream, destination: SocketAddr) -> Result<(), TransportFailure> {
    let mut request = vec![5, 1, 0];
    match destination.ip() {
        std::net::IpAddr::V4(address) => {
            request.push(1);
            request.extend_from_slice(&address.octets());
        }
        std::net::IpAddr::V6(address) => {
            request.push(4);
            request.extend_from_slice(&address.octets());
        }
    }
    request.extend_from_slice(&destination.port().to_be_bytes());
    stream.write_all(&request).await.map_err(map_io_error)?;
    if read_byte(stream).await? != 5 {
        return Err(TransportFailure::ConnectionReset);
    }
    let reply = read_byte(stream).await?;
    if read_byte(stream).await? != 0 {
        return Err(TransportFailure::ConnectionReset);
    }
    if reply != 0 {
        return Err(match reply {
            7 | 8 => TransportFailure::UnsupportedTopology,
            _ => TransportFailure::ConnectionReset,
        });
    }
    match read_byte(stream).await? {
        1 => discard(stream, 6).await,
        4 => discard(stream, 18).await,
        3 => {
            let length = read_byte(stream).await? as usize;
            discard(stream, length + 2).await
        }
        _ => Err(TransportFailure::ConnectionReset),
    }
}

async fn discard(stream: &mut TcpStream, length: usize) -> Result<(), TransportFailure> {
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await.map_err(map_io_error)?;
    Ok(())
}

async fn read_byte(stream: &mut TcpStream) -> Result<u8, TransportFailure> {
    let mut byte = [0];
    stream.read_exact(&mut byte).await.map_err(map_io_error)?;
    Ok(byte[0])
}

fn map_io_error(error: io::Error) -> TransportFailure {
    match error.kind() {
        io::ErrorKind::TimedOut => TransportFailure::ConnectTimeout,
        _ => TransportFailure::ConnectionReset,
    }
}
