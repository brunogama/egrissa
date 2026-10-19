use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{
        EndpointId, NetworkIdentityId, ObservationId, OperatorId, RequestId, RouteId, WorkloadId,
    },
    policy::EgressPolicy,
    request::{
        Destination, DestinationKey, EgressRequest, GeographyRequirement, IdentityRequirement,
        IsolationRequirement, PrivacyRequirement, TransportRequirement,
    },
};
use identity_core::{
    continuity::IdentityContinuity,
    identity::{IdentityScope, NetworkIdentity},
    lifecycle::IdentityState,
};
use observation_core::{
    EgressObservation, Observation, ObservationScope, ObservationSnapshot, Outcome,
};
use route_core::{
    planner::{
        Candidate, DestinationEligibility, DeterministicPlanner, Endpoint, FirstCandidate,
        InventorySnapshot, PlanContext, RoutePlanner,
    },
    topology::RouteTopology,
};
use std::{
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use verification_core::{
    EgressVerifier, EvidenceVerifier, VerificationOutcome, VerificationSubject, VerifiedEgress,
    VerifiedProperties, assess_continuity,
};

fn subject(now: Instant) -> VerificationSubject {
    let mut identity = NetworkIdentity::allocate(
        NetworkIdentityId(1),
        WorkloadId(1),
        IdentityScope::Workload,
        now,
        None,
    )
    .unwrap();
    identity
        .transition(identity.id(), 0, IdentityState::Active, now)
        .unwrap();
    let request = EgressRequest {
        id: RequestId(1),
        workload: WorkloadId(1),
        destination: Destination {
            key: DestinationKey(1),
            port: 443,
        },
        identity: IdentityRequirement::Ephemeral,
        transport: TransportRequirement::Tcp,
        geography: GeographyRequirement::Any,
        isolation: IsolationRequirement::Shared,
        privacy: PrivacyRequirement::Standard,
    };
    let inventory = InventorySnapshot {
        endpoints: vec![Endpoint {
            id: EndpointId(1),
            operator: OperatorId(1),
            enabled: true,
            available: true,
            destinations: DestinationEligibility::Any,
            capabilities: EgressCapabilities {
                transport: TransportCapabilities {
                    tcp: true,
                    ..Default::default()
                },
                identity: IdentityCapabilities {
                    sticky: true,
                    continuity_observable: true,
                    ..Default::default()
                },
                topology: TopologyCapabilities {
                    supported: vec![TopologyKind::Relay],
                    ..Default::default()
                },
                isolation: IsolationCapabilities {
                    shared: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            valid_until: Some(now + Duration::from_secs(60)),
            credential: None,
        }],
        upstreams: vec![],
        candidates: vec![Candidate {
            topology: RouteTopology::Relay {
                relay: EndpointId(1),
            },
            rank: 0,
        }],
        credentials: vec![],
    };
    let policy = EgressPolicy::default();
    let plan = DeterministicPlanner::default()
        .plan(
            &PlanContext {
                request: &request,
                policy: &policy,
                identity: &identity,
                inventory: &inventory,
                observations: &ObservationSnapshot::default(),
                now,
                route: RouteId(1),
                route_generation: 0,
            },
            &mut FirstCandidate,
        )
        .unwrap();
    VerificationSubject {
        id: egress_core::ids::VerificationId(1),
        request,
        plan,
        identity,
        observations: vec![],
        now,
    }
}

fn verify(subject: VerificationSubject) -> VerificationOutcome {
    let mut future = EvidenceVerifier.verify(subject);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(outcome) => outcome,
        Poll::Pending => panic!("evidence verification must not suspend"),
    }
}

#[test]
fn repeated_exit_ip_does_not_prove_identity_continuity() {
    let now = Instant::now();
    let mut subject = subject(now);
    let key = subject.plan.key();
    subject.observations = vec![
        Observation {
            id: ObservationId(1),
            key,
            observed_at: now,
            scope: ObservationScope::Route(key.route),
            outcome: Outcome::EgressObserved(EgressObservation {
                exit_address: Some("203.0.113.7".parse().unwrap()),
                asn: Some(64_496),
                ..Default::default()
            }),
        },
        Observation {
            id: ObservationId(2),
            key,
            observed_at: now + Duration::from_secs(1),
            scope: ObservationScope::Route(key.route),
            outcome: Outcome::EgressObserved(EgressObservation {
                exit_address: Some("203.0.113.7".parse().unwrap()),
                asn: Some(64_496),
                ..Default::default()
            }),
        },
    ];

    let VerificationOutcome::Verified(verified) = verify(subject) else {
        panic!("expected verified egress properties");
    };
    assert_eq!(
        verified.properties.exit_address.unwrap().to_string(),
        "203.0.113.7"
    );
    assert_eq!(verified.properties.asn, Some(64_496));
    assert_eq!(
        verified.properties.identity_continuity,
        IdentityContinuity::Unknown
    );
}

#[test]
fn verifier_specific_preserved_continuity_remains_assessable() {
    let verified = VerifiedEgress {
        id: egress_core::ids::VerificationId(1),
        key: subject(Instant::now()).plan.key(),
        verified_at: Instant::now(),
        properties: VerifiedProperties {
            identity_continuity: IdentityContinuity::Preserved,
            ..Default::default()
        },
        evidence: vec![ObservationId(1)],
    };

    let assessment = assess_continuity(&verified);
    assert_eq!(assessment.continuity, IdentityContinuity::Preserved);
    assert_eq!(assessment.evidence, vec![ObservationId(1)]);
}
