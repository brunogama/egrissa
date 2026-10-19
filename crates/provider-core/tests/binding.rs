use egress_core::{
    capability::{EgressCapabilities, IdentityCapabilities},
    ids::{CredentialRef, OperatorId, ProviderId, UpstreamBindingId},
};
use provider_core::{
    BindingAvailability, BindingLifecycle, BindingObservation, BindingObservationKind,
    ProviderBinding, ProviderInventory,
};
use route_core::planner::{CredentialLease, DestinationEligibility};
use std::time::{Duration, Instant};

fn binding(valid_until: Option<Instant>) -> ProviderBinding {
    ProviderBinding {
        id: UpstreamBindingId(1),
        provider: ProviderId(2),
        operator: OperatorId(3),
        lifecycle: BindingLifecycle::Active { valid_until },
        destinations: DestinationEligibility::Any,
        capabilities: EgressCapabilities {
            identity: IdentityCapabilities {
                sticky: true,
                maximum_sticky_ttl: Some(Duration::from_secs(60)),
                continuity_observable: false,
            },
            ..Default::default()
        },
        credential: Some(CredentialRef(4)),
    }
}

#[test]
fn active_binding_materializes_opaque_capabilities_and_validity() {
    let now = Instant::now();
    let snapshot = ProviderInventory {
        bindings: vec![binding(Some(now + Duration::from_secs(30)))],
        credentials: vec![CredentialLease {
            reference: CredentialRef(4),
            valid_until: Some(now + Duration::from_secs(20)),
        }],
    }
    .materialize(&[], now);

    assert_eq!(snapshot.upstreams.len(), 1);
    assert!(snapshot.upstreams[0].available);
    assert_eq!(snapshot.upstreams[0].credential, Some(CredentialRef(4)));
    assert!(snapshot.upstreams[0].capabilities.identity.sticky);
    assert_eq!(
        snapshot.upstreams[0].valid_until,
        Some(now + Duration::from_secs(30))
    );
    assert_eq!(snapshot.credentials[0].reference, CredentialRef(4));
}

#[test]
fn lifecycle_and_observations_determine_availability_without_mutating_binding() {
    let now = Instant::now();
    let active = binding(Some(now + Duration::from_secs(30)));
    let retired = ProviderBinding {
        lifecycle: BindingLifecycle::Retired,
        ..active.clone()
    };
    let observations = [
        BindingObservation {
            binding: active.id,
            observed_at: now,
            kind: BindingObservationKind::Unavailable,
        },
        BindingObservation {
            binding: active.id,
            observed_at: now + Duration::from_secs(1),
            kind: BindingObservationKind::Available,
        },
        BindingObservation {
            binding: active.id,
            observed_at: now,
            kind: BindingObservationKind::Cooldown {
                until: now + Duration::from_secs(10),
            },
        },
    ];

    assert_eq!(
        BindingAvailability::project(active.id, &observations, now),
        BindingAvailability::CoolingDown {
            until: now + Duration::from_secs(10)
        }
    );
    assert!(
        !ProviderInventory {
            bindings: vec![active],
            credentials: vec![]
        }
        .materialize(&observations, now)
        .upstreams[0]
            .available
    );
    assert!(
        ProviderInventory {
            bindings: vec![retired],
            credentials: vec![]
        }
        .materialize(&observations, now)
        .upstreams
        .is_empty()
    );
    assert!(
        ProviderInventory {
            bindings: vec![binding(Some(now))],
            credentials: vec![]
        }
        .materialize(&[], now)
        .upstreams
        .is_empty()
    );
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn observation_projection_is_input_order_independent(events in prop::collection::vec((any::<bool>(), 0u8..20), 0..40)) {
        let now = Instant::now();
        let id = UpstreamBindingId(1);
        let mut observations = events
            .into_iter()
            .map(|(available, seconds)| BindingObservation {
                binding: id,
                observed_at: now + Duration::from_secs(u64::from(seconds)),
                kind: if available {
                    BindingObservationKind::Available
                } else {
                    BindingObservationKind::Unavailable
                },
            })
            .collect::<Vec<_>>();
        let projected = BindingAvailability::project(id, &observations, now + Duration::from_secs(20));
        observations.reverse();
        prop_assert_eq!(projected, BindingAvailability::project(id, &observations, now + Duration::from_secs(20)));
    }
}
