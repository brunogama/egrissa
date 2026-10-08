//! Immutable runtime evidence, attribution, and health projections.

pub mod attribution;
pub mod health;
pub mod observation;

pub use attribution::{Confidence, FailureAttribution, FailureCategory, FailureScope, attribute};
pub use health::{HealthPolicy, HealthProjection, ObservationSnapshot};
pub use observation::{EgressObservation, Observation, ObservationScope, Outcome};
