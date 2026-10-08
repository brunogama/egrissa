# M0 implementation report

M0 is a three-crate Rust 2024 workspace with no real network I/O. It implements
the semantic/control-plane kernel only. The required source layout is present,
with an additional `egress-core/src/evidence.rs` for passive evidence contracts.

## Implemented invariants

| CRAFT invariant | Enforcement and executable coverage |
| --- | --- |
| Desired, allocated, established, observed, and verified are separate | Private immutable `RoutePlan`, explicit establishment completion, passive observations and verifier-output records; observation-isolation tests and compile-fail conversion tests |
| Identity is not an IP address | `NetworkIdentity` contains logical IDs, scope, generation, lifecycle, and validity only |
| Unknown continuity remains unknown | Explicit continuity assessments; unknown defaults; replacement property and unit tests |
| Route and identity generations are independent | Route replacement increments route generation only; identity replacement increments identity generation explicitly; recovery tests |
| Stale generations cannot mutate current state | Full `FeedbackKey` comparisons before route mutation; identity ID/generation checks before transitions; stale-feedback properties, late-callback tests, and stale or ahead identity generation recovery tests |
| No silent direct fallback | No direct topology exists in M0; no-candidate typed failure; fail-closed property, acquisition-boundary leakage test, and failed-acquisition recovery tests |
| Application status does not mutate health | Recording observations never changes lifecycle or health; arbitrary HTTP status property |
| Provider claims are not verification | Capabilities and observations have no conversion to `VerifiedEgress`; two compile-fail tests |
| Unsupported transport/topology fails before establishment | Planner support list, per-hop role capabilities, transport intersection, TCP-only HTTP CONNECT and no IP-over-SOCKS constraint; positive coverage of all five shapes |
| Identity-changing recovery is explicit | Policy-authorized identity replacement and `ReplacementDecision::ReplaceIdentity`; preserved-continuity rejection tests |
| Route expiry is bounded by all dependencies | Policy, identity, endpoints, upstream binding, credential leases, and sticky TTL bounds; six-independent-bound property and credential tests |
| One establishment per route generation | Atomic synchronous reservation; pending acquisition while establishing; eight-thread test; duplicate-completion and retry-budget tests |
| Credentials are references only | Plans/inventory hold typed `CredentialRef` and validity metadata; no secret field or credential resolver |
| Planning is deterministic except an injected chooser | Stable rank/topology ordering; chooser sees only the equivalent best-rank group; permutation property, custom chooser test, and typed out-of-range failure |

Additional checks reject duplicate inventory identifiers, reused dedicated identities,
expired leases, disabled dependencies, unhealthy/cooling endpoints, unsatisfied destination,
isolation, geography, operator separation, and sticky-lifetime requirements.

---

## Tests and verification

All 41 executable tests pass, including ten `proptest` properties, along with
two compile-fail documentation tests. Root integration targets are explicitly wired
into `route-core` because a virtual workspace does not discover root `tests/` itself.

All three required exit gates passed on Rust 1.98.1:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The ten properties also passed 2,048 cases each with `PROPTEST_RNG_SEED=424242`.
The declared minimum Rust version is 1.85; that older toolchain was not separately tested.

Time and IDs are supplied explicitly. Tests use standard-library threads, a barrier,
and a host mutex for concurrent acquisition, never sockets or an async runtime.
The manager cannot await or invoke a transport. Establishment happens after returning
an `Establish` decision and releasing the caller's mutation lock.

---

## Deliberate deviations from the specification

- `PlanContext` adds the allocated identity, explicit time, and caller-selected route ID/generation to the illustrative planner signature. The planner generates neither IDs nor time internally.
- `route-core` depends on `identity-core` to inspect a real identity snapshot instead of duplicating the identity model. This adds an edge to the recommended dependency diagram, without a cycle or transport dependency.
- Inventory supplies explicit supported route shapes. M0 filters/ranks these candidates; it does not enumerate arbitrary endpoint combinations or implement provider inventory discovery.
- The unspecified supporting policy/value types use conservative definitions. Isolation policy uses the ordering Shared, PerWorkload, PerSession, Dedicated; exact adapter support is still required. `Isolated` requires a dedicated identity and adapter capability.
- `AllowDirect` does not introduce a direct route. `RequireBeforeUse` returns `VerificationUnavailable`, rather than treating capability claims or establishment as proof.
- `Observation` and `FailureAttribution` are passive contracts in `egress-core`. `VerifiedEgress` and `VerifiedProperties` are passive contracts beside continuity in `identity-core`, pending the later observation/verification crates. No attribution or verification engine is implemented.
- The host supplies explicit continuity assessments to recovery. M0 neither produces nor independently validates continuity evidence. Unknown cannot authorize a preserving replan. Production orchestration must accept `Preserved` only from verified evidence.
- Acquire atomically plans/reserves/starts a generation. Unallocated and Planned are transient conceptual states, not independently exposed mutation operations. A retry consumes a fresh route generation and requires a preserving assessment, retaining one establishment per generation.
- An identity in `Allocating` may also transition to `Invalidated`. The specification diagram shows only `Allocating -> Active`; an aborted allocation still needs a terminal state, and the additional edge is covered by the lifecycle property test.

---

## Unresolved design questions

These do not block M0 but must be decided before production orchestration:

1. What evidence and trust boundaries authorize a preserved continuity assessment, and how long is that evidence valid?
2. Should inventory build route combinations itself, and which per-role capability rules should each future adapter advertise?
3. How should destination keys and country/region codes be validated and mapped outside the kernel without leaking destination information?
4. What are the exact production identity-affinity, retry-budget reset, identity retirement, observation retention, and route draining semantics?
5. What verifier implementation and lease linkage should replace the fail-closed `RequireBeforeUse` placeholder?

No SOCKS5, CONNECT, MASQUE, relay networking, DNS, provider adapters, database,
credential storage, or fake end-to-end runtime was added. Implementation stops at M0.
