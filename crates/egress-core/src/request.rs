use std::time::Duration;

use crate::ids::{NetworkIdentityId, RequestId, WorkloadId};

/// An opaque destination key. M0 never resolves or opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestinationKey(pub u128);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub key: DestinationKey,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CountryCode(pub [u8; 2]);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionCode(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Tcp,
    Udp,
    Ip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransportSet {
    pub tcp: bool,
    pub udp: bool,
    pub ip: bool,
}
impl TransportSet {
    pub fn contains(self, transport: Transport) -> bool {
        match transport {
            Transport::Tcp => self.tcp,
            Transport::Udp => self.udp,
            Transport::Ip => self.ip,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRequirement {
    Ephemeral,
    Sticky { minimum_lifetime: Duration },
    Existing(NetworkIdentityId),
    Isolated,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportRequirement {
    Tcp,
    Udp,
    Ip,
    AnyOf(TransportSet),
}
impl TransportRequirement {
    pub fn accepts(self, transport: Transport) -> bool {
        match self {
            Self::Tcp => transport == Transport::Tcp,
            Self::Udp => transport == Transport::Udp,
            Self::Ip => transport == Transport::Ip,
            Self::AnyOf(set) => set.contains(transport),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationRequirement {
    Shared,
    PerSession,
    PerWorkload,
    Dedicated,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeographyRequirement {
    Any,
    Country(CountryCode),
    Region(RegionCode),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyRequirement {
    Standard,
    SeparateClientAndDestinationKnowledge,
    DistinctOperators,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressRequest {
    pub id: RequestId,
    pub workload: WorkloadId,
    pub destination: Destination,
    pub identity: IdentityRequirement,
    pub transport: TransportRequirement,
    pub geography: GeographyRequirement,
    pub isolation: IsolationRequirement,
    pub privacy: PrivacyRequirement,
}
impl EgressRequest {
    /// Request IDs correlate calls; all semantic requirements must still match.
    pub fn compatible_with(&self, other: &Self) -> bool {
        let pinned = matches!(
            self.isolation,
            IsolationRequirement::PerSession | IsolationRequirement::Dedicated
        ) || self.identity == IdentityRequirement::Isolated;
        (!pinned || self.id == other.id)
            && self.workload == other.workload
            && self.destination == other.destination
            && self.identity == other.identity
            && self.transport == other.transport
            && self.geography == other.geography
            && self.isolation == other.isolation
            && self.privacy == other.privacy
    }
}
