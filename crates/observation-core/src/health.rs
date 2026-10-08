use crate::attribution::{Confidence, FailureAttribution, FailureCategory, FailureScope};
use egress_core::ids::{EndpointId, UpstreamBindingId};
use std::time::{Duration, Instant};

/// Health is a projection of attributed evidence, not a mutation of observations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservationSnapshot {
    pub unavailable_endpoints: Vec<EndpointId>,
    pub unavailable_upstreams: Vec<UpstreamBindingId>,
    pub endpoint_cooldowns: Vec<(EndpointId, Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthPolicy {
    pub rate_limit_cooldown: Duration,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            rate_limit_cooldown: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HealthProjection {
    unavailable_endpoints: Vec<EndpointId>,
    unavailable_upstreams: Vec<UpstreamBindingId>,
    endpoint_cooldowns: Vec<(EndpointId, Instant)>,
}

impl HealthProjection {
    pub fn project(
        attributions: &[FailureAttribution],
        now: Instant,
        policy: HealthPolicy,
    ) -> Self {
        let mut projection = Self::default();
        for attribution in attributions {
            match (
                attribution.category,
                attribution.confidence,
                &attribution.affected_scope,
            ) {
                (
                    FailureCategory::EndpointUnavailable,
                    Confidence::Strong | Confidence::Confirmed,
                    FailureScope::Endpoint(id),
                ) => push_unique(&mut projection.unavailable_endpoints, *id),
                (
                    FailureCategory::UpstreamAuthentication,
                    Confidence::Confirmed,
                    FailureScope::Upstream(id),
                ) => push_unique(&mut projection.unavailable_upstreams, *id),
                (
                    FailureCategory::RateLimitSuspected,
                    Confidence::Weak | Confidence::Strong | Confidence::Confirmed,
                    FailureScope::Endpoint(id),
                ) => {
                    if let Some(until) = now.checked_add(policy.rate_limit_cooldown) {
                        match projection
                            .endpoint_cooldowns
                            .iter_mut()
                            .find(|(cooldown_id, _)| cooldown_id == id)
                        {
                            Some((_, existing)) => *existing = (*existing).max(until),
                            None => projection.endpoint_cooldowns.push((*id, until)),
                        }
                    }
                }
                _ => {}
            }
        }
        projection.unavailable_endpoints.sort();
        projection.unavailable_upstreams.sort();
        projection.endpoint_cooldowns.sort_by_key(|(id, _)| *id);
        projection
    }

    pub fn into_snapshot(self) -> ObservationSnapshot {
        ObservationSnapshot {
            unavailable_endpoints: self.unavailable_endpoints,
            unavailable_upstreams: self.unavailable_upstreams,
            endpoint_cooldowns: self.endpoint_cooldowns,
        }
    }
}

fn push_unique<T: Copy + PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_projection_requires_attributed_confidence_and_scope() {
        let now = Instant::now();
        let attributions = [
            FailureAttribution {
                category: FailureCategory::EndpointUnavailable,
                confidence: Confidence::Weak,
                evidence: vec![],
                affected_scope: FailureScope::Endpoint(EndpointId(1)),
            },
            FailureAttribution {
                category: FailureCategory::EndpointUnavailable,
                confidence: Confidence::Strong,
                evidence: vec![],
                affected_scope: FailureScope::Endpoint(EndpointId(2)),
            },
            FailureAttribution {
                category: FailureCategory::UpstreamAuthentication,
                confidence: Confidence::Strong,
                evidence: vec![],
                affected_scope: FailureScope::Upstream(UpstreamBindingId(3)),
            },
            FailureAttribution {
                category: FailureCategory::UpstreamAuthentication,
                confidence: Confidence::Confirmed,
                evidence: vec![],
                affected_scope: FailureScope::Upstream(UpstreamBindingId(4)),
            },
        ];
        let projected = HealthProjection::project(&attributions, now, HealthPolicy::default());
        assert_eq!(projected.unavailable_endpoints, vec![EndpointId(2)]);
        assert_eq!(projected.unavailable_upstreams, vec![UpstreamBindingId(4)]);
    }
}
