//! Verification contracts and evidence-backed egress properties.

use egress_core::{ids::*, request::EgressRequest};
use identity_core::{continuity::IdentityContinuity, identity::NetworkIdentity};
use observation_core::{Observation, Outcome};
use route_core::route::RoutePlan;
use std::{future::Future, net::IpAddr, pin::Pin, time::Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEgress {
    pub id: VerificationId,
    pub key: FeedbackKey,
    pub verified_at: Instant,
    pub properties: VerifiedProperties,
    pub evidence: Vec<ObservationId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifiedProperties {
    pub exit_address: Option<IpAddr>,
    pub asn: Option<u32>,
    pub country: Option<egress_core::request::CountryCode>,
    pub transport: Option<egress_core::request::Transport>,
    pub identity_continuity: IdentityContinuity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationSubject {
    pub id: VerificationId,
    pub request: EgressRequest,
    pub plan: RoutePlan,
    pub identity: NetworkIdentity,
    pub observations: Vec<Observation>,
    pub now: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationFailure {
    Contradicted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    Verified(VerifiedEgress),
    Failed(VerificationFailure),
    InsufficientEvidence,
}

pub type VerificationFuture = Pin<Box<dyn Future<Output = VerificationOutcome> + Send + 'static>>;

pub trait EgressVerifier: Send + Sync + 'static {
    fn verify(&self, subject: VerificationSubject) -> VerificationFuture;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityAssessment {
    pub continuity: IdentityContinuity,
    pub evidence: Vec<ObservationId>,
}

pub fn assess_continuity(verified: &VerifiedEgress) -> ContinuityAssessment {
    let continuity = verified.properties.identity_continuity;
    let evidence = if continuity == IdentityContinuity::Preserved {
        verified.evidence.clone()
    } else {
        Vec::new()
    };
    ContinuityAssessment {
        continuity,
        evidence,
    }
}

/// Verifies only properties present in current-generation egress observations.
#[derive(Debug, Clone, Copy, Default)]
pub struct EvidenceVerifier;

impl EgressVerifier for EvidenceVerifier {
    fn verify(&self, subject: VerificationSubject) -> VerificationFuture {
        Box::pin(async move {
            let evidence: Vec<_> = subject
                .observations
                .iter()
                .filter(|observation| {
                    observation.key == subject.plan.key()
                        && matches!(observation.outcome, Outcome::EgressObserved(_))
                })
                .collect();
            if evidence.is_empty() {
                return VerificationOutcome::InsufficientEvidence;
            }
            let latest = evidence
                .iter()
                .filter_map(|observation| match &observation.outcome {
                    Outcome::EgressObserved(value) => Some((*observation, value)),
                    _ => None,
                })
                .max_by_key(|(observation, _)| observation.observed_at);
            let Some((_, latest_properties)) = latest else {
                return VerificationOutcome::InsufficientEvidence;
            };
            let continuity_evidence = evidence
                .iter()
                .filter_map(|observation| match &observation.outcome {
                    Outcome::EgressObserved(value) => value
                        .exit_address
                        .map(|address| (observation.observed_at, observation.id, address)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let distinct_times = continuity_evidence
                .iter()
                .map(|(observed_at, _, _)| *observed_at)
                .collect::<std::collections::BTreeSet<_>>();
            let continuity = if continuity_evidence.len() >= 2
                && distinct_times.len() >= 2
                && latest_properties.exit_address.is_some()
                && continuity_evidence
                    .iter()
                    .all(|(_, _, address)| Some(*address) == latest_properties.exit_address)
                && subject
                    .observations
                    .iter()
                    .filter(|observation| observation.key.route == subject.plan.id())
                    .all(|observation| {
                        observation.key.identity == subject.identity.id()
                            && observation.key.identity_generation == subject.identity.generation()
                    }) {
                IdentityContinuity::Preserved
            } else {
                IdentityContinuity::Unknown
            };
            let mut ids = evidence
                .iter()
                .map(|observation| observation.id)
                .collect::<Vec<_>>();
            ids.sort();
            VerificationOutcome::Verified(VerifiedEgress {
                id: subject.id,
                key: subject.plan.key(),
                verified_at: subject.now,
                properties: VerifiedProperties {
                    exit_address: latest_properties.exit_address,
                    asn: latest_properties.asn,
                    country: latest_properties.country,
                    transport: latest_properties.transport,
                    identity_continuity: continuity,
                },
                evidence: ids,
            })
        })
    }
}

/// Provider claims and raw observations cannot be promoted directly to verification.
///
/// ```compile_fail
/// use verification_core::VerifiedEgress;
/// use egress_core::capability::EgressCapabilities;
/// let verified: VerifiedEgress = EgressCapabilities::default().into();
/// ```
///
/// ```compile_fail
/// use verification_core::VerifiedEgress;
/// use observation_core::Observation;
/// fn promote(observation: Observation) -> VerifiedEgress { observation.into() }
/// ```
const _: () = ();
