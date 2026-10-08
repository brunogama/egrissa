use crate::observation::{Observation, ObservationScope, Outcome};
use egress_core::{ids::*, request::DestinationKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    TransportFailure,
    EndpointUnavailable,
    ProviderUnavailable,
    UpstreamAuthentication,
    DestinationRejected,
    RateLimitSuspected,
    IdentityReputationSuspected,
    VerificationFailure,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Unknown,
    Weak,
    Strong,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureScope {
    Transport,
    Endpoint(EndpointId),
    Operator(OperatorId),
    Provider(ProviderId),
    Upstream(UpstreamBindingId),
    Destination(DestinationKey),
    Route(RouteId),
    Identity(NetworkIdentityId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureAttribution {
    pub category: FailureCategory,
    pub confidence: Confidence,
    pub evidence: Vec<ObservationId>,
    pub affected_scope: FailureScope,
}

struct Candidate {
    category: FailureCategory,
    confidence: Confidence,
    scope: FailureScope,
    evidence: Vec<ObservationId>,
}

/// Attribute typed evidence without treating destination responses as global health.
pub fn attribute(observations: &[Observation]) -> Option<FailureAttribution> {
    let mut candidates: Vec<Candidate> = Vec::new();
    for observation in observations {
        if let Some((category, confidence, scope)) = classify(observation) {
            if let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.category == category && candidate.scope == scope)
            {
                candidate.evidence.push(observation.id);
                if candidate.confidence == Confidence::Weak {
                    candidate.confidence = Confidence::Strong;
                }
            } else {
                candidates.push(Candidate {
                    category,
                    confidence,
                    scope,
                    evidence: vec![observation.id],
                });
            }
        }
    }
    candidates
        .into_iter()
        .max_by(|a, b| {
            a.confidence
                .cmp(&b.confidence)
                .then_with(|| b.evidence[0].cmp(&a.evidence[0]))
        })
        .map(|candidate| FailureAttribution {
            category: candidate.category,
            confidence: candidate.confidence,
            evidence: candidate.evidence,
            affected_scope: candidate.scope,
        })
}

fn classify(observation: &Observation) -> Option<(FailureCategory, Confidence, FailureScope)> {
    let scope = match &observation.scope {
        ObservationScope::Transport => FailureScope::Transport,
        ObservationScope::Endpoint(id) => FailureScope::Endpoint(*id),
        ObservationScope::Operator(id) => FailureScope::Operator(*id),
        ObservationScope::Provider(id) => FailureScope::Provider(*id),
        ObservationScope::Upstream(id) => FailureScope::Upstream(*id),
        ObservationScope::Destination(id) => FailureScope::Destination(*id),
        ObservationScope::Route(id) => FailureScope::Route(*id),
        ObservationScope::Identity(id) => FailureScope::Identity(*id),
    };
    let (category, confidence) = match (&observation.scope, &observation.outcome) {
        (ObservationScope::Upstream(_), Outcome::AuthenticationRejected) => (
            FailureCategory::UpstreamAuthentication,
            Confidence::Confirmed,
        ),
        (ObservationScope::Endpoint(_), Outcome::AuthenticationRejected) => {
            (FailureCategory::EndpointUnavailable, Confidence::Strong)
        }
        (_, Outcome::ConnectTimeout | Outcome::ConnectionReset) => {
            (FailureCategory::TransportFailure, Confidence::Weak)
        }
        (ObservationScope::Destination(_), Outcome::HttpStatus(429)) => {
            (FailureCategory::RateLimitSuspected, Confidence::Weak)
        }
        (ObservationScope::Destination(_), Outcome::HttpStatus(status))
            if (400..500).contains(status) =>
        {
            (FailureCategory::DestinationRejected, Confidence::Weak)
        }
        (ObservationScope::Route(_), Outcome::VerificationFailed(_)) => {
            (FailureCategory::VerificationFailure, Confidence::Confirmed)
        }
        _ => return None,
    };
    Some((category, confidence, scope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::{EgressObservation, Outcome};
    use std::time::Instant;

    fn observation(id: u128, scope: ObservationScope, outcome: Outcome) -> Observation {
        Observation {
            id: ObservationId(id),
            key: FeedbackKey {
                route: RouteId(1),
                route_generation: 0,
                identity: NetworkIdentityId(1),
                identity_generation: 0,
            },
            observed_at: Instant::now(),
            scope,
            outcome,
        }
    }

    #[test]
    fn destination_http_403_is_attributed_to_destination() {
        let evidence = [observation(
            1,
            ObservationScope::Destination(DestinationKey(1)),
            Outcome::HttpStatus(403),
        )];
        let result = attribute(&evidence).unwrap();
        assert_eq!(result.category, FailureCategory::DestinationRejected);
        assert_eq!(
            result.affected_scope,
            FailureScope::Destination(DestinationKey(1))
        );
        assert_eq!(result.evidence, vec![ObservationId(1)]);
        let projection = crate::health::HealthProjection::project(
            &[result],
            Instant::now(),
            crate::health::HealthPolicy::default(),
        );
        assert!(projection.into_snapshot().unavailable_endpoints.is_empty());
    }

    #[test]
    fn upstream_authentication_is_attributed_to_upstream() {
        let evidence = [observation(
            1,
            ObservationScope::Upstream(UpstreamBindingId(2)),
            Outcome::AuthenticationRejected,
        )];
        let result = attribute(&evidence).unwrap();
        assert_eq!(result.category, FailureCategory::UpstreamAuthentication);
        assert_eq!(result.confidence, Confidence::Confirmed);
        assert_eq!(
            result.affected_scope,
            FailureScope::Upstream(UpstreamBindingId(2))
        );
    }

    #[test]
    fn evidence_without_failure_has_no_attribution() {
        let evidence = [observation(
            1,
            ObservationScope::Transport,
            Outcome::EgressObserved(EgressObservation::default()),
        )];
        assert_eq!(attribute(&evidence), None);
    }
}
