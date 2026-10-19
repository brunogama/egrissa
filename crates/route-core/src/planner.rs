use crate::{route::RoutePlan, topology::RouteTopology};
use egress_core::{capability::*, error::PlanningError, ids::*, policy::*, request::*};
use identity_core::identity::NetworkIdentity;
use observation_core::ObservationSnapshot;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestinationEligibility {
    Any,
    Only(Vec<DestinationKey>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub id: EndpointId,
    pub operator: OperatorId,
    pub enabled: bool,
    pub available: bool,
    pub destinations: DestinationEligibility,
    pub capabilities: EgressCapabilities,
    pub valid_until: Option<Instant>,
    pub credential: Option<CredentialRef>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamBinding {
    pub id: UpstreamBindingId,
    pub operator: OperatorId,
    pub provider: ProviderId,
    pub available: bool,
    pub destinations: DestinationEligibility,
    pub capabilities: EgressCapabilities,
    pub valid_until: Option<Instant>,
    pub credential: Option<CredentialRef>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub topology: RouteTopology,
    pub rank: u32,
}
/// Only validity metadata for an opaque credential reference, never the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialLease {
    pub reference: CredentialRef,
    pub valid_until: Option<Instant>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InventorySnapshot {
    pub endpoints: Vec<Endpoint>,
    pub upstreams: Vec<UpstreamBinding>,
    pub candidates: Vec<Candidate>,
    pub credentials: Vec<CredentialLease>,
}
pub trait CandidateChooser {
    /// Index into the nonempty, deterministically sorted best-rank group.
    fn choose(&mut self, count: usize) -> usize;
}
pub struct FirstCandidate;
impl CandidateChooser for FirstCandidate {
    fn choose(&mut self, _count: usize) -> usize {
        0
    }
}

pub struct PlanContext<'a> {
    pub request: &'a EgressRequest,
    pub policy: &'a EgressPolicy,
    pub identity: &'a NetworkIdentity,
    pub inventory: &'a InventorySnapshot,
    pub observations: &'a ObservationSnapshot,
    pub now: Instant,
    pub route: RouteId,
    pub route_generation: u64,
}
pub trait RoutePlanner {
    fn plan(
        &self,
        context: &PlanContext<'_>,
        chooser: &mut impl CandidateChooser,
    ) -> Result<RoutePlan, PlanningError>;
}
#[derive(Debug, Clone)]
pub struct DeterministicPlanner {
    pub supported_topologies: Vec<TopologyKind>,
}
impl Default for DeterministicPlanner {
    fn default() -> Self {
        Self {
            supported_topologies: vec![
                TopologyKind::Socks5,
                TopologyKind::HttpConnect,
                TopologyKind::Relay,
                TopologyKind::RelayThenSocks5,
                TopologyKind::TwoRelaysThenSocks5,
            ],
        }
    }
}
impl RoutePlanner for DeterministicPlanner {
    fn plan(
        &self,
        context: &PlanContext<'_>,
        chooser: &mut impl CandidateChooser,
    ) -> Result<RoutePlan, PlanningError> {
        validate_context(context)?;
        if !identity_compatible(context) {
            return Err(PlanningError::IncompatibleIdentity);
        }
        let maximum_expiry = context
            .now
            .checked_add(context.policy.routing.maximum_lifetime)
            .ok_or(PlanningError::TimeOverflow)?;
        let mut eligible: Vec<_> = context
            .inventory
            .candidates
            .iter()
            .filter_map(|candidate| {
                self.evaluate(context, candidate, maximum_expiry)
                    .map(|plan| (candidate.rank, plan))
            })
            .collect();
        eligible.sort_by(|a, b| (a.0, &a.1.topology).cmp(&(b.0, &b.1.topology)));
        eligible.dedup_by(|a, b| a.1.topology == b.1.topology);
        let best = eligible
            .first()
            .ok_or(PlanningError::NoEligibleCandidate)?
            .0;
        let count = eligible
            .iter()
            .take_while(|candidate| candidate.0 == best)
            .count();
        let index = chooser.choose(count);
        if index >= count {
            return Err(PlanningError::InvalidChoice { index, count });
        }
        Ok(eligible.remove(index).1)
    }
}

impl DeterministicPlanner {
    pub(crate) fn still_eligible(&self, context: &PlanContext<'_>, plan: &RoutePlan) -> bool {
        if validate_context(context).is_err() {
            return false;
        }
        let Some(maximum) = context
            .now
            .checked_add(context.policy.routing.maximum_lifetime)
        else {
            return false;
        };
        let candidate = Candidate {
            topology: plan.topology().clone(),
            rank: 0,
        };
        let Some(current) = self.evaluate(context, &candidate, maximum) else {
            return false;
        };
        if let IdentityRequirement::Sticky { minimum_lifetime } = context.request.identity {
            if context
                .now
                .checked_add(minimum_lifetime)
                .is_none_or(|end| end > plan.expires_at())
            {
                return false;
            }
        }
        current.transport() == plan.transport()
            && current.expires_at() >= plan.expires_at()
            && current.credentials() == plan.credentials()
    }
    fn evaluate(
        &self,
        context: &PlanContext<'_>,
        candidate: &Candidate,
        maximum_expiry: Instant,
    ) -> Option<RoutePlan> {
        if !self
            .supported_topologies
            .contains(&candidate.topology.kind())
        {
            return None;
        }
        let isolation = required_isolation(context);
        let mut expires_at = maximum_expiry;
        if let Some(end) = context.identity.valid_until() {
            expires_at = expires_at.min(end);
        }
        let mut credentials = Vec::new();
        let mut capabilities = Vec::new();
        let mut operators = Vec::new();
        let mut kinds = Vec::new();
        let mut endpoint_ids = Vec::new();
        for (id, kind) in candidate.topology.endpoints() {
            if endpoint_ids.contains(&id) {
                return None;
            }
            endpoint_ids.push(id);
            let endpoint = context
                .inventory
                .endpoints
                .iter()
                .find(|endpoint| endpoint.id == id)?;
            if !endpoint.enabled
                || !endpoint.available
                || !destination_allowed(&endpoint.destinations, context.request.destination.key)
                || !endpoint.capabilities.topology.supported.contains(&kind)
                || !supports_isolation(&endpoint.capabilities.isolation, isolation)
                || context.observations.unavailable_endpoints.contains(&id)
                || context
                    .observations
                    .endpoint_cooldowns
                    .iter()
                    .any(|&(cooled, until)| cooled == id && context.now < until)
            {
                return None;
            }
            if let Some(end) = endpoint.valid_until {
                expires_at = expires_at.min(end);
            }
            if let Some(credential) = endpoint.credential {
                credentials.push(credential);
            }
            capabilities.push(&endpoint.capabilities);
            operators.push(endpoint.operator);
            kinds.push(kind);
        }
        if let Some(id) = candidate.topology.upstream() {
            let upstream = context
                .inventory
                .upstreams
                .iter()
                .find(|upstream| upstream.id == id)?;
            if !upstream.available
                || !destination_allowed(&upstream.destinations, context.request.destination.key)
                || !supports_isolation(&upstream.capabilities.isolation, isolation)
                || context.observations.unavailable_upstreams.contains(&id)
            {
                return None;
            }
            if let Some(end) = upstream.valid_until {
                expires_at = expires_at.min(end);
            }
            if let Some(credential) = upstream.credential {
                credentials.push(credential);
            }
            capabilities.push(&upstream.capabilities);
            operators.push(upstream.operator);
        }
        for reference in &credentials {
            let lease = context
                .inventory
                .credentials
                .iter()
                .find(|lease| lease.reference == *reference)?;
            if let Some(end) = lease.valid_until {
                expires_at = expires_at.min(end);
            }
        }
        let transport = [Transport::Tcp, Transport::Udp, Transport::Ip]
            .into_iter()
            .find(|&transport| {
                context.request.transport.accepts(transport)
                    && capabilities
                        .iter()
                        .all(|caps| caps.transport.supports(transport))
                    && kinds.iter().enumerate().all(|(index, &kind)| match kind {
                        TopologyKind::HttpConnect => match transport {
                            Transport::Tcp => capabilities[index].transport.http_connect,
                            Transport::Udp => {
                                capabilities[index].transport.connect_udp
                                    && (capabilities[index].transport.http3
                                        || capabilities[index].transport.http2)
                            }
                            Transport::Ip => {
                                capabilities[index].transport.connect_ip
                                    && (capabilities[index].transport.http3
                                        || capabilities[index].transport.http2)
                            }
                        },
                        TopologyKind::Socks5 => transport != Transport::Ip,
                        TopologyKind::Relay
                        | TopologyKind::RelayThenSocks5
                        | TopologyKind::TwoRelaysThenSocks5 => transport == Transport::Tcp,
                    })
            })?;
        if context.policy.routing.distinct_operators
            || context.request.privacy == PrivacyRequirement::DistinctOperators
        {
            let unique: std::collections::BTreeSet<_> = operators.iter().collect();
            if operators.len() < 2 || unique.len() != operators.len() {
                return None;
            }
        }
        if (context.policy.routing.separate_client_and_destination
            || context.request.privacy == PrivacyRequirement::SeparateClientAndDestinationKnowledge)
            && (!candidate.topology.separates_knowledge()
                || !capabilities
                    .iter()
                    .all(|caps| caps.topology.separate_client_and_destination))
        {
            return None;
        }
        let exit = capabilities.last()?;
        let geographic_match = match &context.request.geography {
            GeographyRequirement::Any => true,
            GeographyRequirement::Country(country) => exit.geography.countries.contains(country),
            GeographyRequirement::Region(region) => exit.geography.regions.contains(region),
        };
        if !geographic_match {
            return None;
        }
        if context.policy.identity.require_observable_continuity
            && !exit.identity.continuity_observable
        {
            return None;
        }
        if let IdentityRequirement::Sticky { minimum_lifetime } = context.request.identity {
            if !exit.identity.sticky {
                return None;
            }
            if let Some(ttl) = exit.identity.maximum_sticky_ttl {
                expires_at = expires_at.min(context.now.checked_add(ttl)?);
            }
            if context.now.checked_add(minimum_lifetime)? > expires_at {
                return None;
            }
        }
        if expires_at <= context.now {
            return None;
        }
        Some(RoutePlan {
            key: FeedbackKey {
                route: context.route,
                route_generation: context.route_generation,
                identity: context.identity.id(),
                identity_generation: context.identity.generation(),
            },
            request: context.request.id,
            topology: candidate.topology.clone(),
            transport,
            created_at: context.now,
            expires_at,
            credentials,
        })
    }
}

pub(crate) fn required_isolation(context: &PlanContext<'_>) -> IsolationRequirement {
    if context.request.identity == IdentityRequirement::Isolated {
        IsolationRequirement::Dedicated
    } else {
        stronger_isolation(context.request.isolation, context.policy.isolation.minimum)
    }
}

pub(crate) fn stronger_isolation(
    a: IsolationRequirement,
    b: IsolationRequirement,
) -> IsolationRequirement {
    fn rank(value: IsolationRequirement) -> u8 {
        match value {
            IsolationRequirement::Shared => 0,
            IsolationRequirement::PerWorkload => 1,
            IsolationRequirement::PerSession => 2,
            IsolationRequirement::Dedicated => 3,
        }
    }
    if rank(a) >= rank(b) { a } else { b }
}
fn supports_isolation(caps: &IsolationCapabilities, requirement: IsolationRequirement) -> bool {
    match requirement {
        IsolationRequirement::Shared => caps.shared,
        IsolationRequirement::PerSession => caps.per_session,
        IsolationRequirement::PerWorkload => caps.per_workload,
        IsolationRequirement::Dedicated => caps.dedicated,
    }
}
fn destination_allowed(eligibility: &DestinationEligibility, key: DestinationKey) -> bool {
    match eligibility {
        DestinationEligibility::Any => true,
        DestinationEligibility::Only(keys) => keys.contains(&key),
    }
}

pub(crate) fn identity_compatible(context: &PlanContext<'_>) -> bool {
    use identity_core::identity::IdentityScope;
    let identity = context.identity;
    if !identity.is_usable(context.now) || identity.workload() != context.request.workload {
        return false;
    }
    match context.request.identity {
        IdentityRequirement::Existing(id) if identity.id() != id => return false,
        IdentityRequirement::Isolated if identity.scope() != IdentityScope::Dedicated => {
            return false;
        }
        _ => {}
    }
    match required_isolation(context) {
        IsolationRequirement::Shared => true,
        IsolationRequirement::PerWorkload => matches!(
            identity.scope(),
            IdentityScope::Workload | IdentityScope::Dedicated
        ),
        IsolationRequirement::PerSession => matches!(
            identity.scope(),
            IdentityScope::Session | IdentityScope::Dedicated
        ),
        IsolationRequirement::Dedicated => identity.scope() == IdentityScope::Dedicated,
    }
}

pub(crate) fn validate_context(context: &PlanContext<'_>) -> Result<(), PlanningError> {
    use std::collections::BTreeSet;
    let inventory = context.inventory;
    if inventory
        .endpoints
        .iter()
        .map(|endpoint| endpoint.id)
        .collect::<BTreeSet<_>>()
        .len()
        != inventory.endpoints.len()
        || inventory
            .upstreams
            .iter()
            .map(|upstream| upstream.id)
            .collect::<BTreeSet<_>>()
            .len()
            != inventory.upstreams.len()
        || inventory
            .credentials
            .iter()
            .map(|lease| lease.reference)
            .collect::<BTreeSet<_>>()
            .len()
            != inventory.credentials.len()
    {
        return Err(PlanningError::InvalidInventory);
    }
    Ok(())
}
