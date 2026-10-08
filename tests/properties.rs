mod support;
use egress_core::{capability::*, error::*, evidence::*, ids::*, policy::*, request::*};
use identity_core::{
    continuity::IdentityContinuity,
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use proptest::prelude::*;
use route_core::{lifecycle::*, planner::*, topology::RouteTopology};
use std::time::{Duration, Instant};
use support::*;

proptest! {
    #[test]
    fn stale_identity_generations_never_mutate_state(generation in 1u64..=u64::MAX, expired in any::<bool>()) {
        let now = Instant::now(); let mut id = identity(now); let original = id.clone();
        let feedback_time = if expired { now + Duration::from_secs(300) } else { now };
        prop_assert_eq!(id.transition(id.id(), generation, IdentityState::Degraded, feedback_time), Err(LifecycleError::StaleFeedback));
        prop_assert_eq!(id, original);
    }
    #[test]
    fn identity_transition_sequences_obey_lifecycle_rules(events in prop::collection::vec(0u8..5, 0..50)) {
        let now = Instant::now();
        let mut id = NetworkIdentity::allocate(NetworkIdentityId(1), WorkloadId(1), IdentityScope::Workload, now, None).unwrap();
        let states = [IdentityState::Allocating, IdentityState::Active, IdentityState::Degraded, IdentityState::Invalidated, IdentityState::Expired];
        for event in events {
            let previous = id.state(); let next = states[event as usize];
            let valid = [(IdentityState::Allocating, IdentityState::Active), (IdentityState::Allocating, IdentityState::Invalidated), (IdentityState::Active, IdentityState::Degraded), (IdentityState::Active, IdentityState::Invalidated), (IdentityState::Degraded, IdentityState::Active), (IdentityState::Degraded, IdentityState::Invalidated)].contains(&(previous, next));
            let result = id.transition(id.id(), id.generation(), next, now);
            prop_assert_eq!(result.is_ok(), valid);
            prop_assert_eq!(id.state(), if valid { next } else { previous });
        }
    }
    #[test]
    fn stale_route_feedback_never_mutates_state(generation in 1u64..=u64::MAX, identity_generation in any::<bool>()) {
        let now = Instant::now(); let req = request(); let policy = EgressPolicy::default(); let id = identity(now); let inv = inventory(now); let health = ObservationSnapshot::default();
        let mut manager = RouteManager::default();
        let AcquireDecision::Establish(plan) = manager.acquire(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate) else { unreachable!() };
        let mut key = plan.key();
        if identity_generation { key.identity_generation = generation; } else { key.route_generation = generation; }
        prop_assert_eq!(manager.complete(key, EstablishmentResult::Established, plan.expires_at()), Err(LifecycleError::StaleFeedback));
        prop_assert_eq!(manager.state(plan.id()), Some(RouteState::Establishing));
    }
    #[test]
    fn route_expiry_is_bounded_by_every_dependency(bounds in prop::array::uniform6(1u64..1000)) {
        let now = Instant::now(); let req = request(); let mut policy = EgressPolicy::default(); policy.routing.maximum_lifetime = Duration::from_secs(bounds[0]);
        let mut id = NetworkIdentity::allocate(NetworkIdentityId(1), WorkloadId(1), IdentityScope::Workload, now, Some(now + Duration::from_secs(bounds[1]))).unwrap();
        id.transition(id.id(), 0, IdentityState::Active, now).unwrap();
        let mut inv = inventory(now); inv.endpoints[0].valid_until = Some(now + Duration::from_secs(bounds[2]));
        let mut gateway = inv.endpoints[0].clone(); gateway.id = EndpointId(2); gateway.capabilities.topology.supported = vec![TopologyKind::Socks5]; gateway.valid_until = Some(now + Duration::from_secs(bounds[3])); inv.endpoints.push(gateway);
        inv.upstreams.push(UpstreamBinding { id: UpstreamBindingId(1), operator: OperatorId(3), provider: ProviderId(1), available: true, destinations: DestinationEligibility::Any, capabilities: capabilities(), valid_until: Some(now + Duration::from_secs(bounds[4])), credential: None });
        inv.credentials[0].valid_until = Some(now + Duration::from_secs(bounds[5]));
        inv.candidates[0].topology = RouteTopology::RelayThenSocks5 { relay: EndpointId(1), gateway: EndpointId(2), upstream: UpstreamBindingId(1) };
        let health = ObservationSnapshot::default();
        let plan = DeterministicPlanner::default().plan(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate).unwrap();
        for bound in bounds { prop_assert!(plan.expires_at() <= now + Duration::from_secs(bound)); }
        prop_assert!(plan.is_valid(now)); prop_assert!(!plan.is_valid(plan.expires_at()));
    }
    #[test]
    fn distinct_operator_policy_never_emits_equal_operators(a in 0u128..8, b in 0u128..8) {
        let now = Instant::now(); let mut req = request(); req.privacy = PrivacyRequirement::DistinctOperators;
        let policy = EgressPolicy::default(); let id = identity(now); let mut inv = inventory(now); let health = ObservationSnapshot::default();
        inv.endpoints[0].operator = OperatorId(a); inv.endpoints[0].capabilities.topology.supported = vec![TopologyKind::Socks5];
        inv.upstreams.push(UpstreamBinding { id: UpstreamBindingId(1), operator: OperatorId(b), provider: ProviderId(1), available: true, destinations: DestinationEligibility::Any, capabilities: capabilities(), valid_until: None, credential: None });
        inv.candidates[0].topology = RouteTopology::Socks5 { gateway: EndpointId(1), upstream: UpstreamBindingId(1) };
        let result = DeterministicPlanner::default().plan(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate);
        prop_assert_eq!(result.is_ok(), a != b);
    }
    #[test]
    fn planner_never_emits_unsupported_transport(tcp in any::<bool>(), udp in any::<bool>(), ip in any::<bool>(), required in 0u8..3) {
        let now = Instant::now(); let mut req = request(); let policy = EgressPolicy::default(); let id = identity(now); let mut inv = inventory(now); let health = ObservationSnapshot::default();
        inv.endpoints[0].capabilities.transport = TransportCapabilities { tcp, udp, ip, ..Default::default() };
        req.transport = [TransportRequirement::Tcp, TransportRequirement::Udp, TransportRequirement::Ip][required as usize];
        let result = DeterministicPlanner::default().plan(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate);
        prop_assert_eq!(result.is_ok(), [tcp, udp, ip][required as usize]);
    }
    #[test]
    fn identity_replacement_cannot_report_preserved(new_id in 2u128..=u128::MAX, continuity in 0u8..3) {
        let now = Instant::now(); let mut id = identity(now);
        let assessment = [IdentityContinuity::Preserved, IdentityContinuity::Changed, IdentityContinuity::Unknown][continuity as usize];
        let result = id.replace(RecoveryPolicy::ReplanAllowingIdentityChange, NetworkIdentityId(new_id), assessment, now, None);
        match result {
            Ok(replacement) => { prop_assert_ne!(replacement.continuity, IdentityContinuity::Preserved); prop_assert_eq!(replacement.new.generation(), 1); }
            Err(error) => prop_assert_eq!(error, RecoveryError::InvalidContinuity),
        }
    }
    #[test]
    fn arbitrary_http_observations_never_mutate_lifecycle(statuses in prop::collection::vec(any::<u16>(), 0..50)) {
        let now = Instant::now(); let req = request(); let policy = EgressPolicy::default(); let id = identity(now); let inv = inventory(now); let health = ObservationSnapshot::default();
        let mut manager = RouteManager::default();
        let AcquireDecision::Establish(plan) = manager.acquire(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate) else { unreachable!() };
        for (index, status) in statuses.into_iter().enumerate() {
            manager.record_observation(Observation { id: ObservationId(index as u128), key: plan.key(), observed_at: now, scope: ObservationScope::Endpoint(EndpointId(1)), outcome: Outcome::HttpStatus(status) }).unwrap();
            prop_assert_eq!(manager.state(plan.id()), Some(RouteState::Establishing));
        }
    }
    #[test]
    fn deterministic_inputs_produce_deterministic_ranking(ranks in prop::collection::vec(any::<u32>(), 1..10)) {
        let now = Instant::now(); let req = request(); let policy = EgressPolicy::default(); let id = identity(now); let mut inv = inventory(now); let health = ObservationSnapshot::default();
        inv.candidates.clear(); let endpoint = inv.endpoints[0].clone(); inv.endpoints.clear();
        for (index, rank) in ranks.into_iter().enumerate() {
            let mut endpoint = endpoint.clone(); endpoint.id = EndpointId(index as u128); inv.candidates.push(Candidate { topology: RouteTopology::Relay { relay: endpoint.id }, rank }); inv.endpoints.push(endpoint);
        }
        let planner = DeterministicPlanner::default();
        let before = planner.plan(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate);
        inv.endpoints.reverse(); inv.candidates.reverse();
        prop_assert_eq!(before, planner.plan(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate));
    }
    #[test]
    fn fail_closed_never_returns_direct_or_weakens_transport(enabled in any::<bool>(), supported in any::<bool>()) {
        let now = Instant::now(); let req = request(); let policy = EgressPolicy::default(); let id = identity(now); let mut inv = inventory(now); let health = ObservationSnapshot::default();
        inv.endpoints[0].enabled = enabled; inv.endpoints[0].capabilities.transport.tcp = supported;
        let result = DeterministicPlanner::default().plan(&context(&req, &policy, &id, &inv, &health, now), &mut FirstCandidate);
        if let Ok(plan) = result { prop_assert_eq!(plan.topology(), &RouteTopology::Relay { relay: EndpointId(1) }); prop_assert_eq!(plan.transport(), Transport::Tcp); } else { prop_assert!(!enabled || !supported); }
    }
}
