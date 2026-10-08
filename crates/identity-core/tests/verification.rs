use identity_core::continuity::IdentityContinuity;

#[test]
fn unknown_continuity_is_not_preserved() {
    assert_eq!(IdentityContinuity::default(), IdentityContinuity::Unknown);
}
