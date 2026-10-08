#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdentityContinuity {
    Preserved,
    Changed,
    #[default]
    Unknown,
}

/// Passive verifier-output contract. No M0 component produces verification.
/// Provider capabilities and observations have no conversion to this type.
///
/// ```compile_fail
/// use identity_core::continuity::VerifiedEgress;
/// use egress_core::capability::EgressCapabilities;
/// let verified: VerifiedEgress = EgressCapabilities::default().into();
/// ```
///
/// ```compile_fail
/// use identity_core::continuity::VerifiedEgress;
/// use egress_core::evidence::Observation;
/// fn promote(observation: Observation) -> VerifiedEgress { observation.into() }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEgress {
    pub id: egress_core::ids::VerificationId,
    pub key: egress_core::ids::FeedbackKey,
    pub verified_at: std::time::Instant,
    pub properties: VerifiedProperties,
    pub evidence: Vec<egress_core::ids::ObservationId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifiedProperties {
    pub exit_address: Option<std::net::IpAddr>,
    pub asn: Option<u32>,
    pub country: Option<egress_core::request::CountryCode>,
    pub transport: Option<egress_core::request::Transport>,
    pub identity_continuity: IdentityContinuity,
}
