use egress_core::{capability::TopologyKind, ids::*};

/// Explicit v1 shapes. Direct access is deliberately not an M0 topology.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteTopology {
    Socks5 {
        gateway: EndpointId,
        upstream: UpstreamBindingId,
    },
    HttpConnect {
        proxy: EndpointId,
    },
    Relay {
        relay: EndpointId,
    },
    RelayThenSocks5 {
        relay: EndpointId,
        gateway: EndpointId,
        upstream: UpstreamBindingId,
    },
    TwoRelaysThenSocks5 {
        ingress: EndpointId,
        egress: EndpointId,
        gateway: EndpointId,
        upstream: UpstreamBindingId,
    },
}
impl RouteTopology {
    pub fn kind(&self) -> TopologyKind {
        match self {
            Self::Socks5 { .. } => TopologyKind::Socks5,
            Self::HttpConnect { .. } => TopologyKind::HttpConnect,
            Self::Relay { .. } => TopologyKind::Relay,
            Self::RelayThenSocks5 { .. } => TopologyKind::RelayThenSocks5,
            Self::TwoRelaysThenSocks5 { .. } => TopologyKind::TwoRelaysThenSocks5,
        }
    }
    pub fn endpoints(&self) -> Vec<(EndpointId, TopologyKind)> {
        match *self {
            Self::Socks5 { gateway, .. } => vec![(gateway, TopologyKind::Socks5)],
            Self::HttpConnect { proxy } => vec![(proxy, TopologyKind::HttpConnect)],
            Self::Relay { relay } => vec![(relay, TopologyKind::Relay)],
            Self::RelayThenSocks5 { relay, gateway, .. } => vec![
                (relay, TopologyKind::Relay),
                (gateway, TopologyKind::Socks5),
            ],
            Self::TwoRelaysThenSocks5 {
                ingress,
                egress,
                gateway,
                ..
            } => vec![
                (ingress, TopologyKind::Relay),
                (egress, TopologyKind::Relay),
                (gateway, TopologyKind::Socks5),
            ],
        }
    }
    pub fn upstream(&self) -> Option<UpstreamBindingId> {
        match *self {
            Self::Socks5 { upstream, .. }
            | Self::RelayThenSocks5 { upstream, .. }
            | Self::TwoRelaysThenSocks5 { upstream, .. } => Some(upstream),
            Self::HttpConnect { .. } | Self::Relay { .. } => None,
        }
    }
    pub fn separates_knowledge(&self) -> bool {
        matches!(
            self,
            Self::RelayThenSocks5 { .. } | Self::TwoRelaysThenSocks5 { .. }
        )
    }
}
