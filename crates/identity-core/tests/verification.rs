use identity_core::continuity::{IdentityContinuity, VerifiedProperties};

#[test]
fn unverified_properties_and_continuity_remain_unknown() {
    let properties = VerifiedProperties::default();
    assert_eq!(properties.exit_address, None);
    assert_eq!(properties.country, None);
    assert_eq!(properties.transport, None);
    assert_eq!(properties.identity_continuity, IdentityContinuity::Unknown);
}
