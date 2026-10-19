#[path = "../../../tests/support/mod.rs"]
mod support;
use egress_core::{capability::*, ids::*, policy::*};
use route_core::{planner::*, topology::RouteTopology};
use std::time::Instant;
use support::*;

#[test]
fn every_v1_topology_has_an_explicit_supported_plan() {
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let health = ObservationSnapshot::default();
    let mut inv = inventory(now);
    inv.endpoints[0].capabilities.transport.http_connect = true;
    inv.endpoints[0].capabilities.topology.supported = vec![
        TopologyKind::Relay,
        TopologyKind::Socks5,
        TopologyKind::HttpConnect,
    ];
    for n in 2..=3 {
        let mut endpoint = inv.endpoints[0].clone();
        endpoint.id = EndpointId(n);
        endpoint.operator = OperatorId(n);
        inv.endpoints.push(endpoint);
    }
    inv.upstreams.push(UpstreamBinding {
        id: UpstreamBindingId(1),
        operator: OperatorId(4),
        provider: ProviderId(1),
        available: true,
        destinations: DestinationEligibility::Any,
        capabilities: capabilities(),
        valid_until: None,
        credential: None,
    });
    let topologies = [
        RouteTopology::Socks5 {
            gateway: EndpointId(1),
            upstream: UpstreamBindingId(1),
        },
        RouteTopology::HttpConnect {
            proxy: EndpointId(1),
        },
        RouteTopology::Relay {
            relay: EndpointId(1),
        },
        RouteTopology::RelayThenSocks5 {
            relay: EndpointId(1),
            gateway: EndpointId(2),
            upstream: UpstreamBindingId(1),
        },
        RouteTopology::TwoRelaysThenSocks5 {
            ingress: EndpointId(1),
            egress: EndpointId(2),
            gateway: EndpointId(3),
            upstream: UpstreamBindingId(1),
        },
    ];
    for topology in topologies {
        inv.candidates = vec![Candidate {
            topology: topology.clone(),
            rank: 0,
        }];
        let plan = DeterministicPlanner::default()
            .plan(
                &context(&req, &policy, &id, &inv, &health, now),
                &mut FirstCandidate,
            )
            .unwrap();
        assert_eq!(plan.topology(), &topology);
    }
}
#[test]
fn chooser_only_selects_among_best_rank_candidates() {
    struct LastEquivalent;
    impl CandidateChooser for LastEquivalent {
        fn choose(&mut self, count: usize) -> usize {
            assert_eq!(count, 2);
            count - 1
        }
    }
    let now = Instant::now();
    let req = request();
    let policy = EgressPolicy::default();
    let id = identity(now);
    let health = ObservationSnapshot::default();
    let mut inv = inventory(now);
    for n in 2..=3 {
        let mut endpoint = inv.endpoints[0].clone();
        endpoint.id = EndpointId(n);
        inv.endpoints.push(endpoint);
        inv.candidates.push(Candidate {
            topology: RouteTopology::Relay {
                relay: EndpointId(n),
            },
            rank: if n == 3 { 1 } else { 0 },
        });
    }
    inv.candidates.reverse();
    let plan = DeterministicPlanner::default()
        .plan(
            &context(&req, &policy, &id, &inv, &health, now),
            &mut LastEquivalent,
        )
        .unwrap();
    assert_eq!(
        plan.topology(),
        &RouteTopology::Relay {
            relay: EndpointId(2)
        }
    );
}

#[test]
fn relay_topologies_reject_udp_before_establishment() {
    let now = Instant::now();
    let mut req = request();
    req.transport = egress_core::request::TransportRequirement::Udp;
    let policy = EgressPolicy::default();
    let id = identity(now);
    let health = ObservationSnapshot::default();
    let inv = inventory(now);

    assert!(
        DeterministicPlanner::default()
            .plan(
                &context(&req, &policy, &id, &inv, &health, now),
                &mut FirstCandidate,
            )
            .is_err()
    );
}

#[test]
fn relay_planning_rejects_same_operator_hops_when_separation_is_required() {
    let now = Instant::now();
    let req = request();
    let mut policy = EgressPolicy::default();
    policy.routing.distinct_operators = true;
    let id = identity(now);
    let health = ObservationSnapshot::default();
    let mut inv = inventory(now);
    inv.endpoints[0].capabilities.topology.supported =
        vec![TopologyKind::RelayThenSocks5, TopologyKind::Socks5];
    let mut gateway = inv.endpoints[0].clone();
    gateway.id = EndpointId(2);
    inv.endpoints.push(gateway);
    inv.upstreams.push(UpstreamBinding {
        id: UpstreamBindingId(1),
        operator: OperatorId(2),
        provider: ProviderId(1),
        available: true,
        destinations: DestinationEligibility::Any,
        capabilities: capabilities(),
        valid_until: None,
        credential: None,
    });
    inv.candidates = vec![Candidate {
        topology: RouteTopology::RelayThenSocks5 {
            relay: EndpointId(1),
            gateway: EndpointId(2),
            upstream: UpstreamBindingId(1),
        },
        rank: 0,
    }];

    assert!(
        DeterministicPlanner::default()
            .plan(
                &context(&req, &policy, &id, &inv, &health, now),
                &mut FirstCandidate,
            )
            .is_err()
    );
}
