use crate::{planner::CandidateChooser, topology::RouteTopology};
use egress_core::{
    ids::{EndpointId, ObservationId, UpstreamBindingId},
    request::DestinationKey,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reliability(u16);

impl Reliability {
    pub const MAX: u16 = 1_000;

    pub fn new(value: u16) -> Self {
        Self(value.min(Self::MAX))
    }

    pub fn value(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliabilityScope {
    Endpoint(EndpointId),
    Upstream(UpstreamBindingId),
    Transport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedReliability {
    pub scope: ReliabilityScope,
    pub value: Reliability,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatencyEvidence {
    pub duration: Duration,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooldownScope {
    Endpoint(EndpointId),
    Upstream(UpstreamBindingId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CooldownEvidence {
    pub scope: CooldownScope,
    pub until: Instant,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cost(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationEvidence {
    pub destination: DestinationKey,
    pub penalty: u64,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteScoreInput {
    pub topology: RouteTopology,
    pub static_rank: u32,
    pub reliability: Vec<ScopedReliability>,
    pub latency: Option<LatencyEvidence>,
    pub cooldowns: Vec<CooldownEvidence>,
    pub cost: Cost,
    pub destination_evidence: Vec<DestinationEvidence>,
}

impl RouteScoreInput {
    pub fn neutral(topology: RouteTopology, static_rank: u32) -> Self {
        Self {
            topology,
            static_rank,
            reliability: vec![],
            latency: None,
            cooldowns: vec![],
            cost: Cost(0),
            destination_evidence: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionPolicy {
    #[default]
    Deterministic,
    AdaptiveWithinEqualScore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RankingPolicy {
    pub selection: SelectionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteIntelligence {
    pub policy: RankingPolicy,
    pub candidates: Vec<RouteScoreInput>,
}

impl RouteIntelligence {
    pub fn input_for(&self, topology: &RouteTopology, static_rank: u32) -> RouteScoreInput {
        self.candidates
            .iter()
            .find(|input| input.topology == *topology)
            .map(|input| RouteScoreInput {
                topology: input.topology.clone(),
                static_rank,
                reliability: input.reliability.clone(),
                latency: input.latency.clone(),
                cooldowns: input.cooldowns.clone(),
                cost: input.cost,
                destination_evidence: input.destination_evidence.clone(),
            })
            .unwrap_or_else(|| RouteScoreInput::neutral(topology.clone(), static_rank))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreBreakdown {
    pub reliability_penalty: u16,
    pub latency_penalty: u128,
    pub cost_penalty: u64,
    pub destination_penalty: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedRoute {
    pub topology: RouteTopology,
    pub static_rank: u32,
    pub total_penalty: u128,
    pub breakdown: ScoreBreakdown,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionReason {
    ActiveCooldown { until: Instant },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedRoute {
    pub topology: RouteTopology,
    pub static_rank: u32,
    pub reason: ExclusionReason,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionReason {
    Deterministic,
    Adaptive,
    DeterministicFallback,
    NoEligibleRoute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecisionRecord {
    pub ranked: Vec<RankedRoute>,
    pub excluded: Vec<ExcludedRoute>,
    pub selected: Option<RouteTopology>,
    pub selection: SelectionReason,
}

pub fn decide(
    destination: DestinationKey,
    now: Instant,
    inputs: &[RouteScoreInput],
    policy: RankingPolicy,
    chooser: &mut impl CandidateChooser,
) -> RouteDecisionRecord {
    let mut ranked = Vec::new();
    let mut excluded = Vec::new();

    for input in inputs {
        let mut evidence = input
            .reliability
            .iter()
            .flat_map(|item| item.evidence.iter().copied())
            .chain(
                input
                    .latency
                    .iter()
                    .flat_map(|item| item.evidence.iter().copied()),
            )
            .chain(
                input
                    .destination_evidence
                    .iter()
                    .filter(|item| item.destination == destination)
                    .flat_map(|item| item.evidence.iter().copied()),
            )
            .collect::<Vec<_>>();
        evidence.sort();
        evidence.dedup();

        if let Some((until, cooldown_evidence)) = active_cooldown(input, now) {
            let mut cooldown_evidence = cooldown_evidence;
            cooldown_evidence.sort();
            cooldown_evidence.dedup();
            excluded.push(ExcludedRoute {
                topology: input.topology.clone(),
                static_rank: input.static_rank,
                reason: ExclusionReason::ActiveCooldown { until },
                evidence: cooldown_evidence,
            });
            continue;
        }

        let reliability = input
            .reliability
            .iter()
            .map(|item| item.value)
            .min()
            .unwrap_or(Reliability::new(Reliability::MAX));
        let latency_penalty = input
            .latency
            .as_ref()
            .map_or(0, |latency| latency.duration.as_micros());
        let destination_penalty = input
            .destination_evidence
            .iter()
            .filter(|item| item.destination == destination)
            .fold(0_u64, |total, item| total.saturating_add(item.penalty));
        let breakdown = ScoreBreakdown {
            reliability_penalty: Reliability::MAX.saturating_sub(reliability.value()),
            latency_penalty,
            cost_penalty: input.cost.0,
            destination_penalty,
        };
        let total_penalty = u128::from(breakdown.reliability_penalty)
            .saturating_add(breakdown.latency_penalty)
            .saturating_add(u128::from(breakdown.cost_penalty))
            .saturating_add(u128::from(breakdown.destination_penalty));
        ranked.push(RankedRoute {
            topology: input.topology.clone(),
            static_rank: input.static_rank,
            total_penalty,
            breakdown,
            evidence,
        });
    }

    ranked.sort_by(|left, right| {
        (left.total_penalty, left.static_rank, &left.topology).cmp(&(
            right.total_penalty,
            right.static_rank,
            &right.topology,
        ))
    });
    excluded.sort_by(|left, right| {
        (left.static_rank, &left.topology).cmp(&(right.static_rank, &right.topology))
    });

    let (selected, selection) = select(&ranked, policy.selection, chooser);
    RouteDecisionRecord {
        ranked,
        excluded,
        selected,
        selection,
    }
}

fn active_cooldown(input: &RouteScoreInput, now: Instant) -> Option<(Instant, Vec<ObservationId>)> {
    input
        .cooldowns
        .iter()
        .filter(|cooldown| cooldown.until > now)
        .fold(None, |current, cooldown| match current {
            Some((until, mut evidence)) if until >= cooldown.until => {
                evidence.extend(cooldown.evidence.iter().copied());
                Some((until, evidence))
            }
            Some((_, mut evidence)) => {
                evidence.extend(cooldown.evidence.iter().copied());
                Some((cooldown.until, evidence))
            }
            None => Some((cooldown.until, cooldown.evidence.clone())),
        })
}

fn select(
    ranked: &[RankedRoute],
    policy: SelectionPolicy,
    chooser: &mut impl CandidateChooser,
) -> (Option<RouteTopology>, SelectionReason) {
    let Some(first) = ranked.first() else {
        return (None, SelectionReason::NoEligibleRoute);
    };
    if policy == SelectionPolicy::Deterministic {
        return (Some(first.topology.clone()), SelectionReason::Deterministic);
    }

    let count = ranked
        .iter()
        .take_while(|candidate| {
            candidate.total_penalty == first.total_penalty
                && candidate.static_rank == first.static_rank
        })
        .count();
    let index = chooser.choose(count);
    if index < count {
        (
            Some(ranked[index].topology.clone()),
            SelectionReason::Adaptive,
        )
    } else {
        (
            Some(first.topology.clone()),
            SelectionReason::DeterministicFallback,
        )
    }
}
