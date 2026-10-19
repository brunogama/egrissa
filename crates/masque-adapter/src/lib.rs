//! MASQUE adapter contracts. HTTP session ownership remains behind `MasqueConnector`.

use egress_core::{
    ids::{CredentialRef, EndpointId},
    request::{DestinationKey, Transport},
};
use route_core::topology::RouteTopology;
use std::{collections::BTreeSet, future::Future, net::SocketAddr, pin::Pin, sync::Arc};
use transport_core::{
    DataPlaneEstablishment, DataPlaneEstablishmentFuture, DataPlaneTransport, DataPlaneTunnel,
    EstablishmentRequest, TransportFailure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MasqueService {
    ConnectUdp,
    ConnectIp,
}

impl MasqueService {
    pub fn protocol(self) -> &'static str {
        match self {
            Self::ConnectUdp => "connect-udp",
            Self::ConnectIp => "connect-ip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasqueHttpVersion {
    Http3,
    Http2Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasqueAdvertisement {
    http3: BTreeSet<MasqueService>,
    http2_fallback: Option<BTreeSet<MasqueService>>,
}

impl MasqueAdvertisement {
    pub fn new(
        http3: impl IntoIterator<Item = MasqueService>,
        http2_fallback: Option<impl IntoIterator<Item = MasqueService>>,
    ) -> Result<Self, MasqueConfigurationError> {
        let http3 = http3.into_iter().collect::<BTreeSet<_>>();
        if http3.is_empty() {
            return Err(MasqueConfigurationError::MissingHttp3Service);
        }
        let http2_fallback =
            http2_fallback.map(|services| services.into_iter().collect::<BTreeSet<_>>());
        if http2_fallback
            .as_ref()
            .is_some_and(|services| !services.is_subset(&http3))
        {
            return Err(MasqueConfigurationError::Http2FallbackWithoutHttp3);
        }
        Ok(Self {
            http3,
            http2_fallback,
        })
    }

    pub fn select(&self, service: MasqueService) -> Option<MasqueHttpVersion> {
        self.http3
            .contains(&service)
            .then_some(MasqueHttpVersion::Http3)
    }

    fn has_http2_fallback(&self, service: MasqueService) -> bool {
        self.http2_fallback
            .as_ref()
            .is_some_and(|services| services.contains(&service))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasqueConfigurationError {
    MissingHttp3Service,
    Http2FallbackWithoutHttp3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasqueProxy {
    pub address: SocketAddr,
    pub authority: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasqueTarget {
    pub scheme: String,
    pub path: String,
}

impl MasqueTarget {
    pub fn new(scheme: impl Into<String>, path: impl Into<String>) -> Option<Self> {
        let scheme = scheme.into();
        let path = path.into();
        (scheme == "https" && path.starts_with('/')).then_some(Self { scheme, path })
    }
}

pub trait MasqueAddressResolver: Send + Sync + 'static {
    fn proxy(&self, endpoint: EndpointId) -> Option<MasqueProxy>;
    fn target(
        &self,
        destination: DestinationKey,
        port: u16,
        service: MasqueService,
    ) -> Option<MasqueTarget>;
}

pub trait MasqueCredentialResolver: Send + Sync + 'static {
    fn resolve(&self, reference: CredentialRef) -> Option<MasqueCredentials>;
}

#[derive(Clone)]
pub struct MasqueCredentials {
    token: Vec<u8>,
}

impl MasqueCredentials {
    pub fn bearer(token: impl AsRef<[u8]>) -> Self {
        Self {
            token: token.as_ref().to_vec(),
        }
    }

    fn valid(&self) -> bool {
        !self.token.is_empty()
            && !self
                .token
                .iter()
                .any(|byte| matches!(byte, b'\0' | b'\r' | b'\n'))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasqueMethod {
    Connect,
}

#[derive(Clone)]
pub struct MasqueConnectRequest {
    pub version: MasqueHttpVersion,
    pub method: MasqueMethod,
    pub service: MasqueService,
    pub authority: String,
    pub scheme: String,
    pub path: String,
    capsule_protocol: bool,
    credentials: Option<MasqueCredentials>,
}

impl MasqueConnectRequest {
    pub fn has_credentials(&self) -> bool {
        self.credentials.is_some()
    }

    pub fn uses_capsule_protocol(&self) -> bool {
        self.capsule_protocol
    }

    pub fn bearer_token(&self) -> Option<&[u8]> {
        self.credentials
            .as_ref()
            .map(|credentials| credentials.token.as_slice())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasqueAttemptFailure {
    BeforeRequest,
    Rejected,
}

pub type MasqueOpenFuture =
    Pin<Box<dyn Future<Output = Result<DataPlaneTunnel, MasqueAttemptFailure>> + Send + 'static>>;

pub trait MasqueConnector: Send + Sync + 'static {
    fn open(&self, proxy: SocketAddr, request: MasqueConnectRequest) -> MasqueOpenFuture;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiatedMasque {
    pub version: MasqueHttpVersion,
    pub service: MasqueService,
}

pub struct MasqueAdapter<A, C, N> {
    addresses: Arc<A>,
    credentials: Arc<C>,
    connector: Arc<N>,
    advertisement: MasqueAdvertisement,
}

impl<A, C, N> Clone for MasqueAdapter<A, C, N> {
    fn clone(&self) -> Self {
        Self {
            addresses: Arc::clone(&self.addresses),
            credentials: Arc::clone(&self.credentials),
            connector: Arc::clone(&self.connector),
            advertisement: self.advertisement.clone(),
        }
    }
}

impl<A, C, N> MasqueAdapter<A, C, N> {
    pub fn new(
        addresses: A,
        credentials: C,
        connector: N,
        advertisement: MasqueAdvertisement,
    ) -> Self {
        Self {
            addresses: Arc::new(addresses),
            credentials: Arc::new(credentials),
            connector: Arc::new(connector),
            advertisement,
        }
    }
}

impl<A, C, N> MasqueAdapter<A, C, N>
where
    A: MasqueAddressResolver,
    C: MasqueCredentialResolver,
    N: MasqueConnector,
{
    async fn open(
        &self,
        request: EstablishmentRequest,
    ) -> Result<DataPlaneEstablishment<NegotiatedMasque>, TransportFailure> {
        let RouteTopology::HttpConnect { proxy } = request.plan.topology() else {
            return Err(TransportFailure::UnsupportedTopology);
        };
        let service = match request.plan.transport() {
            Transport::Udp => MasqueService::ConnectUdp,
            Transport::Ip => MasqueService::ConnectIp,
            Transport::Tcp => return Err(TransportFailure::UnsupportedTopology),
        };
        let version = self
            .advertisement
            .select(service)
            .ok_or(TransportFailure::UnsupportedTopology)?;
        let proxy = self
            .addresses
            .proxy(*proxy)
            .filter(|proxy| !proxy.authority.is_empty())
            .ok_or(TransportFailure::ConnectionReset)?;
        let target = self
            .addresses
            .target(request.destination.key, request.destination.port, service)
            .ok_or(TransportFailure::ConnectionReset)?;
        let credentials = match request.plan.credentials().first() {
            Some(reference) => Some(
                self.credentials
                    .resolve(*reference)
                    .filter(MasqueCredentials::valid)
                    .ok_or(TransportFailure::AuthenticationRejected)?,
            ),
            None => None,
        };
        let connect = |version| MasqueConnectRequest {
            version,
            method: MasqueMethod::Connect,
            service,
            authority: proxy.authority.clone(),
            scheme: target.scheme.clone(),
            path: target.path.clone(),
            capsule_protocol: true,
            credentials: credentials.clone(),
        };
        match self.connector.open(proxy.address, connect(version)).await {
            Ok(tunnel) => Ok(DataPlaneEstablishment {
                tunnel,
                negotiation: NegotiatedMasque { version, service },
            }),
            Err(MasqueAttemptFailure::BeforeRequest)
                if version == MasqueHttpVersion::Http3
                    && self.advertisement.has_http2_fallback(service) =>
            {
                let version = MasqueHttpVersion::Http2Fallback;
                let tunnel = self
                    .connector
                    .open(proxy.address, connect(version))
                    .await
                    .map_err(|_| TransportFailure::ConnectionReset)?;
                Ok(DataPlaneEstablishment {
                    tunnel,
                    negotiation: NegotiatedMasque { version, service },
                })
            }
            Err(_) => Err(TransportFailure::ConnectionReset),
        }
    }
}

impl<A, C, N> DataPlaneTransport for MasqueAdapter<A, C, N>
where
    A: MasqueAddressResolver,
    C: MasqueCredentialResolver,
    N: MasqueConnector,
{
    type Negotiation = NegotiatedMasque;

    fn establish_data_plane(
        &self,
        request: EstablishmentRequest,
    ) -> DataPlaneEstablishmentFuture<Self::Negotiation> {
        let adapter = self.clone();
        Box::pin(async move { adapter.open(request).await })
    }
}
