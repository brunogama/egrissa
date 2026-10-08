#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdentityContinuity {
    Preserved,
    Changed,
    #[default]
    Unknown,
}
