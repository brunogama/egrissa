#[path = "../../../tests/support/mod.rs"]
mod support;

use egress_core::{
    capability::{
        EgressCapabilities, IdentityCapabilities, IsolationCapabilities, TopologyCapabilities,
        TopologyKind, TransportCapabilities,
    },
    ids::{CredentialRef, OperatorId, ProviderId, UpstreamBindingId},
    request::IdentityRequirement,
};
use provider_core::{
    BindingLifecycle, BindingObservation, BindingObservationKind, ProviderBinding,
};
use reference_provider_adapter::{ReferenceProviderAdapter, ReferenceProviderConfig};
use route_core::{
    planner::{
        Candidate, CredentialLease, DestinationEligibility, DeterministicPlanner, FirstCandidate,
        RoutePlanner,
    },
    topology::RouteTopology,
};
use std::time::{Duration, Instant};

#[test]
fn configurable_provider_publishes_a_generic_sticky_inventory() {
    let now = Instant::now();
    let mut inventory = support::inventory(now);
    inventory.endpoints[0].capabilities.topology.supported = vec![TopologyKind::Socks5];
    inventory.candidates = vec![Candidate {
        topology: RouteTopology::Socks5 {
            gateway: inventory.endpoints[0].id,
            upstream: UpstreamBindingId(9),
        },
        rank: 0,
    }];
    inventory.credentials.push(CredentialLease {
        reference: CredentialRef(3),
        valid_until: Some(now + Duration::from_secs(20)),
    });
    let config = ReferenceProviderConfig {
        binding: ProviderBinding {
            id: UpstreamBindingId(9),
            provider: ProviderId(2),
            operator: OperatorId(3),
            lifecycle: BindingLifecycle::Active {
                valid_until: Some(now + Duration::from_secs(30)),
            },
            destinations: DestinationEligibility::Any,
            capabilities: EgressCapabilities {
                transport: TransportCapabilities {
                    tcp: true,
                    ..Default::default()
                },
                identity: IdentityCapabilities {
                    sticky: true,
                    maximum_sticky_ttl: Some(Duration::from_secs(25)),
                    continuity_observable: true,
                },
                topology: TopologyCapabilities {
                    supported: vec![TopologyKind::Socks5],
                    separate_client_and_destination: false,
                },
                isolation: IsolationCapabilities {
                    shared: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            credential: Some(CredentialRef(3)),
        },
        credentials: inventory.credentials,
        endpoints: inventory.endpoints,
        candidates: inventory.candidates,
    };
    let adapter = ReferenceProviderAdapter::new(config);
    let published = adapter.inventory(&[], now);
    assert_eq!(published.upstreams.len(), 1);
    assert_eq!(published.upstreams[0].credential, Some(CredentialRef(3)));

    let mut request = support::request();
    request.identity = IdentityRequirement::Sticky {
        minimum_lifetime: Duration::from_secs(15),
    };
    let policy = egress_core::policy::EgressPolicy::default();
    let identity = support::identity(now);
    let health = support::ObservationSnapshot::default();
    let plan = DeterministicPlanner::default()
        .plan(
            &support::context(&request, &policy, &identity, &published, &health, now),
            &mut FirstCandidate,
        )
        .unwrap();
    assert_eq!(plan.credentials(), &[CredentialRef(1), CredentialRef(3)]);
    assert_eq!(plan.expires_at(), now + Duration::from_secs(20));

    let unavailable = adapter.inventory(
        &[BindingObservation {
            binding: UpstreamBindingId(9),
            observed_at: now,
            kind: BindingObservationKind::Unavailable,
        }],
        now,
    );
    assert!(
        DeterministicPlanner::default()
            .plan(
                &support::context(&request, &policy, &identity, &unavailable, &health, now),
                &mut FirstCandidate,
            )
            .is_err()
    );
}
