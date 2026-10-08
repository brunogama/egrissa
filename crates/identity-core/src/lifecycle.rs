#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityState {
    Allocating,
    Active,
    Degraded,
    Invalidated,
    Expired,
}
