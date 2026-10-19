mod support;

use egress_core::{ids::*, policy::EgressPolicy, request::DestinationKey};
use proptest::prelude::*;
use route_core::{
    intelligence::{
        CooldownEvidence, CooldownScope, Cost, DestinationEvidence, LatencyEvidence, RankingPolicy,
        Reliability, ReliabilityScope, RouteIntelligence, RouteScoreInput, ScopedReliability,
        SelectionPolicy, SelectionReason, decide,
    },
    planner::{Candidate, DeterministicPlanner, FirstCandidate},
    topology::RouteTopology,
};
use std::time::{Duration, Instant};
use support::*;

fn input(endpoint: u128, static_rank: u32, reliability: u16) -> RouteScoreInput {
    RouteScoreInput {
        topology: RouteTopology::Relay {
            relay: EndpointId(endpoint),
        },
        static_rank,
        reliability: vec![ScopedReliability {
            scope: ReliabilityScope::Endpoint(EndpointId(endpoint)),
            value: Reliability::new(reliability),
            evidence: vec![ObservationId(endpoint)],
        }],
        latency: None,
        cooldowns: vec![],
        cost: Cost(0),
        destination_evidence: vec![],
    }
}

#[test]
fn reliability_outranks_static_rank() {
    let now = Instant::now();
    let decision = decide(
        DestinationKey(1),
        now,
        &[input(1, 0, 100), input(2, 1, 900)],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );

    assert_eq!(decision.selected, Some(relay(2)));
    assert_eq!(decision.ranked[0].breakdown.reliability_penalty, 100);
}

#[test]
fn latency_and_cost_affect_equal_reliability_scores() {
    let now = Instant::now();
    let mut lower_latency = input(1, 0, Reliability::MAX);
    lower_latency.latency = Some(LatencyEvidence {
        duration: Duration::from_micros(100),
        evidence: vec![ObservationId(1)],
    });
    let mut higher_latency = input(2, 1, Reliability::MAX);
    higher_latency.latency = Some(LatencyEvidence {
        duration: Duration::from_micros(1),
        evidence: vec![ObservationId(2)],
    });
    let decision = decide(
        DestinationKey(1),
        now,
        &[lower_latency, higher_latency],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );
    assert_eq!(decision.selected, Some(relay(2)));
    assert_eq!(decision.ranked[0].breakdown.latency_penalty, 1);

    let mut expensive = input(1, 0, Reliability::MAX);
    expensive.cost = Cost(100);
    let decision = decide(
        DestinationKey(1),
        now,
        &[expensive, input(2, 1, Reliability::MAX)],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );
    assert_eq!(decision.selected, Some(relay(2)));
    assert_eq!(decision.ranked[0].breakdown.cost_penalty, 0);
}

#[test]
fn destination_evidence_reorders_only_its_destination() {
    let now = Instant::now();
    let mut penalized = input(1, 0, Reliability::MAX);
    penalized.destination_evidence = vec![DestinationEvidence {
        destination: DestinationKey(2),
        penalty: 1_000,
        evidence: vec![ObservationId(5)],
    }];
    let unaffected = input(2, 1, Reliability::MAX);

    let first_destination = decide(
        DestinationKey(1),
        now,
        &[penalized.clone(), unaffected.clone()],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );
    assert_eq!(first_destination.selected, Some(relay(1)));

    let second_destination = decide(
        DestinationKey(2),
        now,
        &[penalized, unaffected],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );
    assert_eq!(second_destination.selected, Some(relay(2)));
    assert_eq!(
        second_destination.ranked[0].breakdown.destination_penalty,
        0
    );
}

#[test]
fn active_cooldown_is_excluded_with_its_evidence() {
    let now = Instant::now();
    let mut cooled = input(1, 0, Reliability::MAX);
    cooled.cooldowns = vec![CooldownEvidence {
        scope: CooldownScope::Endpoint(EndpointId(1)),
        until: now + Duration::from_secs(1),
        evidence: vec![ObservationId(6)],
    }];
    let decision = decide(
        DestinationKey(1),
        now,
        &[cooled, input(2, 1, Reliability::MAX)],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );

    assert_eq!(decision.selected, Some(relay(2)));
    assert_eq!(decision.excluded.len(), 1);
    assert_eq!(decision.excluded[0].evidence, vec![ObservationId(6)]);
    assert_eq!(decision.excluded[0].topology, relay(1));
    assert_eq!(
        decision.excluded[0].reason,
        route_core::intelligence::ExclusionReason::ActiveCooldown {
            until: now + Duration::from_secs(1)
        }
    );
}

#[test]
fn invalid_adaptive_choice_falls_back_deterministically() {
    struct InvalidChooser;
    impl route_core::planner::CandidateChooser for InvalidChooser {
        fn choose(&mut self, count: usize) -> usize {
            count
        }
    }

    let now = Instant::now();
    let decision = decide(
        DestinationKey(1),
        now,
        &[input(2, 0, Reliability::MAX), input(1, 0, Reliability::MAX)],
        RankingPolicy {
            selection: SelectionPolicy::AdaptiveWithinEqualScore,
        },
        &mut InvalidChooser,
    );

    assert_eq!(decision.selected, Some(relay(1)));
    assert_eq!(decision.selection, SelectionReason::DeterministicFallback);
}

#[test]
fn valid_adaptive_choice_selects_within_the_equal_score_group() {
    struct SecondCandidate;
    impl route_core::planner::CandidateChooser for SecondCandidate {
        fn choose(&mut self, _count: usize) -> usize {
            1
        }
    }

    let decision = decide(
        DestinationKey(1),
        Instant::now(),
        &[input(1, 0, Reliability::MAX), input(2, 0, Reliability::MAX)],
        RankingPolicy {
            selection: SelectionPolicy::AdaptiveWithinEqualScore,
        },
        &mut SecondCandidate,
    );

    assert_eq!(decision.selected, Some(relay(2)));
    assert_eq!(decision.selection, SelectionReason::Adaptive);
}

#[test]
fn empty_score_input_records_no_eligible_route() {
    let decision = decide(
        DestinationKey(1),
        Instant::now(),
        &[],
        RankingPolicy::default(),
        &mut FirstCandidate,
    );

    assert_eq!(decision.selected, None);
    assert_eq!(decision.selection, SelectionReason::NoEligibleRoute);
}
#[test]
fn planner_uses_intelligence_after_hard_eligibility() {
    let now = Instant::now();
    let request = request();
    let policy = EgressPolicy::default();
    let identity = identity(now);
    let mut inventory = inventory(now);
    let mut endpoint = inventory.endpoints[0].clone();
    endpoint.id = EndpointId(2);
    inventory.endpoints.push(endpoint);
    inventory.candidates.push(Candidate {
        topology: relay(2),
        rank: 1,
    });
    let health = ObservationSnapshot::default();
    let intelligence = RouteIntelligence {
        policy: RankingPolicy::default(),
        candidates: vec![input(1, 0, 100), input(2, 1, 900)],
    };

    let decision = DeterministicPlanner::default()
        .plan_with_decision(
            &context(&request, &policy, &identity, &inventory, &health, now),
            &intelligence,
            &mut FirstCandidate,
        )
        .unwrap();

    assert_eq!(decision.plan.topology(), &relay(2));
    assert_eq!(decision.record.selected, Some(relay(2)));
}

#[test]
fn planner_does_not_rank_hard_ineligible_candidates() {
    let now = Instant::now();
    let request = request();
    let policy = EgressPolicy::default();
    let identity = identity(now);
    let mut inventory = inventory(now);
    let mut endpoint = inventory.endpoints[0].clone();
    endpoint.id = EndpointId(2);
    inventory.endpoints.push(endpoint);
    inventory.candidates.push(Candidate {
        topology: relay(2),
        rank: 1,
    });
    inventory.endpoints[0].enabled = false;
    let health = ObservationSnapshot::default();
    let intelligence = RouteIntelligence {
        policy: RankingPolicy::default(),
        candidates: vec![input(1, 0, Reliability::MAX), input(2, 1, 0)],
    };

    let decision = DeterministicPlanner::default()
        .plan_with_decision(
            &context(&request, &policy, &identity, &inventory, &health, now),
            &intelligence,
            &mut FirstCandidate,
        )
        .unwrap();

    assert_eq!(decision.plan.topology(), &relay(2));
    assert_eq!(decision.record.ranked.len(), 1);
    assert_eq!(decision.record.ranked[0].topology, relay(2));
}

proptest! {
    #[test]
    fn input_permutation_does_not_change_deterministic_ranking(ranks in prop::collection::vec(0_u32..10, 1..10)) {
        let now = Instant::now();
        let inputs = ranks
            .iter()
            .enumerate()
            .map(|(index, rank)| input(index as u128 + 1, *rank, Reliability::MAX))
            .collect::<Vec<_>>();
        let mut reversed = inputs.clone();
        reversed.reverse();
        let before = decide(
            DestinationKey(1),
            now,
            &inputs,
            RankingPolicy::default(),
            &mut FirstCandidate,
        );
        let after = decide(
            DestinationKey(1),
            now,
            &reversed,
            RankingPolicy::default(),
            &mut FirstCandidate,
        );
        prop_assert_eq!(before, after);
    }
}

fn relay(endpoint: u128) -> RouteTopology {
    RouteTopology::Relay {
        relay: EndpointId(endpoint),
    }
}
