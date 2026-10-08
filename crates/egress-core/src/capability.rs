use crate::request::*;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TopologyKind {
    Socks5,
    HttpConnect,
    Relay,
    RelayThenSocks5,
    TwoRelaysThenSocks5,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransportCapabilities {
    pub tcp: bool,
    pub udp: bool,
    pub ip: bool,
    pub http_connect: bool,
    pub connect_udp: bool,
    pub connect_ip: bool,
    pub http2: bool,
    pub http3: bool,
}
impl TransportCapabilities {
    pub fn supports(self, transport: Transport) -> bool {
        match transport {
            Transport::Tcp => self.tcp,
            Transport::Udp => self.udp,
            Transport::Ip => self.ip,
        }
    }
    /// Stable preference order for AnyOf: TCP, UDP, IP.
    pub fn select(self, requirement: TransportRequirement) -> Option<Transport> {
        [Transport::Tcp, Transport::Udp, Transport::Ip]
            .into_iter()
            .find(|&transport| requirement.accepts(transport) && self.supports(transport))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdentityCapabilities {
    pub sticky: bool,
    pub maximum_sticky_ttl: Option<Duration>,
    pub continuity_observable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopologyCapabilities {
    pub supported: Vec<TopologyKind>,
    pub separate_client_and_destination: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeographyCapabilities {
    pub countries: Vec<CountryCode>,
    pub regions: Vec<RegionCode>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IsolationCapabilities {
    pub shared: bool,
    pub per_session: bool,
    pub per_workload: bool,
    pub dedicated: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VerificationCapabilities {
    pub exit_address: bool,
    pub geography: bool,
    pub continuity: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EgressCapabilities {
    pub transport: TransportCapabilities,
    pub identity: IdentityCapabilities,
    pub topology: TopologyCapabilities,
    pub geography: GeographyCapabilities,
    pub isolation: IsolationCapabilities,
    pub verification: VerificationCapabilities,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn any_of_requires_at_least_one_supported_transport() {
        let caps = TransportCapabilities {
            tcp: true,
            ..Default::default()
        };
        assert_eq!(
            caps.select(TransportRequirement::AnyOf(TransportSet::default())),
            None
        );
        assert_eq!(caps.select(TransportRequirement::Udp), None);
        assert_eq!(caps.select(TransportRequirement::Tcp), Some(Transport::Tcp));
    }
}
