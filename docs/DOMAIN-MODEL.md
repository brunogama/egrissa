# Domain Model
Canonical objects:
1. `EgressRequest` — requirements, not proxy choice.
2. `NetworkIdentity` — logical continuity, not IP.
3. `EgressPolicy` — identity/routing/isolation/recovery/verification/leakage.
4. `RoutePlan` — immutable desired route.
5. `Observation` — immutable evidence.
6. `FailureAttribution` — evidence-backed interpretation with confidence/scope.
7. `VerifiedEgress` — properties actually proven.

Route and identity generations are independent. Every async callback carries both IDs/generations. Continuity is `Preserved | Changed | Unknown`. A `RouteLease` is bounded permission to use current routing state; it is not verification.

Recovery: `FailClosed | RetrySameRoute | ReplanPreservingIdentity | ReplanAllowingIdentityChange`.
