//! HTTP CONNECT adapter with optional TLS to the proxy.

use egress_core::{
    ids::{CredentialRef, EndpointId},
    request::{DestinationKey, Transport},
};
use route_core::topology::RouteTopology;
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, ServerName},
};
use std::{io, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tokio_rustls::{TlsConnector, client::TlsStream};
use transport_core::{
    EstablishmentRequest, TransportFailure, Tunnel, TunnelCloseFuture, TunnelEstablishmentFuture,
    TunnelReadFuture, TunnelTransport, TunnelWriteFuture,
};

pub trait HttpConnectAddressResolver: Send + Sync + 'static {
    fn proxy_address(&self, proxy: EndpointId) -> Option<SocketAddr>;
    fn destination(&self, key: DestinationKey, port: u16) -> Option<(String, u16)>;
}

pub trait HttpConnectCredentialResolver: Send + Sync + 'static {
    fn resolve(&self, reference: CredentialRef) -> Option<HttpConnectCredentials>;
}

#[derive(Clone)]
pub struct HttpConnectCredentials {
    username: Vec<u8>,
    password: Vec<u8>,
}
impl HttpConnectCredentials {
    pub fn new(username: impl AsRef<[u8]>, password: impl AsRef<[u8]>) -> Self {
        Self {
            username: username.as_ref().to_vec(),
            password: password.as_ref().to_vec(),
        }
    }
    fn valid(&self) -> bool {
        !self.username.is_empty()
            && !self.password.is_empty()
            && !self.username.contains(&b':')
            && !self
                .username
                .iter()
                .chain(&self.password)
                .any(|byte| *byte == b'\r' || *byte == b'\n')
    }
}

#[derive(Clone)]
pub struct TlsToProxy {
    pub server_name: String,
    pub additional_roots: Vec<Vec<u8>>,
}

#[derive(Clone, Copy)]
pub struct HttpConnectTimeouts {
    pub establish: Duration,
    pub io: Duration,
}
impl Default for HttpConnectTimeouts {
    fn default() -> Self {
        Self {
            establish: Duration::from_secs(10),
            io: Duration::from_secs(30),
        }
    }
}

pub struct HttpConnectAdapter<A, C> {
    addresses: Arc<A>,
    credentials: Arc<C>,
    tls: Option<TlsToProxy>,
    timeouts: HttpConnectTimeouts,
}
impl<A, C> Clone for HttpConnectAdapter<A, C> {
    fn clone(&self) -> Self {
        Self {
            addresses: Arc::clone(&self.addresses),
            credentials: Arc::clone(&self.credentials),
            tls: self.tls.clone(),
            timeouts: self.timeouts,
        }
    }
}
impl<A, C> HttpConnectAdapter<A, C> {
    pub fn new(addresses: A, credentials: C) -> Self {
        Self {
            addresses: Arc::new(addresses),
            credentials: Arc::new(credentials),
            tls: None,
            timeouts: HttpConnectTimeouts::default(),
        }
    }
    pub fn with_tls(mut self, tls: TlsToProxy) -> Self {
        self.tls = Some(tls);
        self
    }
    pub fn with_timeouts(mut self, timeouts: HttpConnectTimeouts) -> Self {
        self.timeouts = timeouts;
        self
    }
}

impl<A, C> HttpConnectAdapter<A, C>
where
    A: HttpConnectAddressResolver,
    C: HttpConnectCredentialResolver,
{
    async fn open(
        &self,
        request: EstablishmentRequest,
    ) -> Result<Box<dyn Tunnel>, TransportFailure> {
        let RouteTopology::HttpConnect { proxy } = request.plan.topology() else {
            return Err(TransportFailure::UnsupportedTopology);
        };
        if request.plan.transport() != Transport::Tcp {
            return Err(TransportFailure::UnsupportedTopology);
        }
        let proxy = self
            .addresses
            .proxy_address(*proxy)
            .ok_or(TransportFailure::ConnectionReset)?;
        let destination = self
            .addresses
            .destination(request.destination.key, request.destination.port)
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
            .is_some_and(|credential| !credential.valid())
        {
            return Err(TransportFailure::AuthenticationRejected);
        }
        timeout(
            self.timeouts.establish,
            self.connect(proxy, destination, credentials.as_ref()),
        )
        .await
        .map_err(|_| TransportFailure::ConnectTimeout)?
    }
    async fn connect(
        &self,
        proxy: SocketAddr,
        destination: (String, u16),
        credentials: Option<&HttpConnectCredentials>,
    ) -> Result<Box<dyn Tunnel>, TransportFailure> {
        let tcp = TcpStream::connect(proxy).await.map_err(map_io)?;
        let mut stream = match &self.tls {
            Some(config) => Stream::Tls(Box::new(connect_tls(tcp, config).await?)),
            None => Stream::Plain(tcp),
        };
        let authority = format!("{}:{}", destination.0, destination.1);
        let mut bytes =
            format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n").into_bytes();
        if let Some(credential) = credentials {
            bytes.extend_from_slice(b"Proxy-Authorization: Basic ");
            bytes.extend_from_slice(base64(&credential.username, &credential.password).as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"\r\n");
        stream.write_all(&bytes).await.map_err(map_io)?;
        let status = read_status(&mut stream).await?;
        match status {
            200..=299 => Ok(Box::new(HttpTunnel {
                stream,
                timeout: self.timeouts.io,
            })),
            407 => Err(TransportFailure::AuthenticationRejected),
            _ => Err(TransportFailure::ConnectionReset),
        }
    }
}

impl<A, C> TunnelTransport for HttpConnectAdapter<A, C>
where
    A: HttpConnectAddressResolver,
    C: HttpConnectCredentialResolver,
{
    fn establish_tunnel(&self, request: EstablishmentRequest) -> TunnelEstablishmentFuture {
        let adapter = self.clone();
        Box::pin(async move { adapter.open(request).await })
    }
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}
impl Stream {
    async fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(bytes).await,
            Self::Tls(stream) => stream.read(bytes).await,
        }
    }
    async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.write_all(bytes).await,
            Self::Tls(stream) => stream.write_all(bytes).await,
        }
    }
    async fn shutdown(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.shutdown().await,
            Self::Tls(stream) => stream.shutdown().await,
        }
    }
}
struct HttpTunnel {
    stream: Stream,
    timeout: Duration,
}
impl Tunnel for HttpTunnel {
    fn read(&mut self, maximum: usize) -> TunnelReadFuture<'_> {
        let duration = self.timeout;
        Box::pin(async move {
            let mut bytes = vec![0; maximum];
            let read = timeout(duration, self.stream.read(&mut bytes))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "tunnel read timed out"))??;
            bytes.truncate(read);
            Ok(bytes)
        })
    }
    fn write(&mut self, bytes: Vec<u8>) -> TunnelWriteFuture<'_> {
        let duration = self.timeout;
        Box::pin(async move {
            let length = bytes.len();
            timeout(duration, self.stream.write_all(&bytes))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "tunnel write timed out"))??;
            Ok(length)
        })
    }
    fn close(mut self: Box<Self>) -> TunnelCloseFuture {
        Box::pin(async move { self.stream.shutdown().await })
    }
}

async fn connect_tls(
    stream: TcpStream,
    config: &TlsToProxy,
) -> Result<TlsStream<TcpStream>, TransportFailure> {
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    roots.add_parsable_certificates(native.certs);
    for der in &config.additional_roots {
        roots
            .add(CertificateDer::from(der.clone()))
            .map_err(|_| TransportFailure::ConnectionReset)?;
    }
    let client = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(config.server_name.clone())
        .map_err(|_| TransportFailure::ConnectionReset)?;
    TlsConnector::from(Arc::new(client))
        .connect(server_name, stream)
        .await
        .map_err(|_| TransportFailure::ConnectionReset)
}
async fn read_status(stream: &mut Stream) -> Result<u16, TransportFailure> {
    let mut bytes = Vec::new();
    let mut one = [0];
    while bytes.len() < 8192 {
        stream.read(&mut one).await.map_err(map_io)?;
        bytes.push(one[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            let line = std::str::from_utf8(&bytes)
                .map_err(|_| TransportFailure::ConnectionReset)?
                .lines()
                .next()
                .ok_or(TransportFailure::ConnectionReset)?;
            return line
                .split_whitespace()
                .nth(1)
                .ok_or(TransportFailure::ConnectionReset)?
                .parse()
                .map_err(|_| TransportFailure::ConnectionReset);
        }
    }
    Err(TransportFailure::ConnectionReset)
}
fn map_io(error: io::Error) -> TransportFailure {
    if error.kind() == io::ErrorKind::TimedOut {
        TransportFailure::ConnectTimeout
    } else {
        TransportFailure::ConnectionReset
    }
}
fn base64(username: &[u8], password: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut input = username.to_vec();
    input.push(b':');
    input.extend_from_slice(password);
    let mut output = String::new();
    for chunk in input.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(TABLE[((value >> 18) & 63) as usize] as char);
        output.push(TABLE[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}
