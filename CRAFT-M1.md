# Craft — Egress Runtime M1: Fake End-to-End Runtime

## Prerequisite
M0 must compile and pass `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. Fix M0 first without weakening its invariants.

## Objective
Implement the first complete orchestration path using a deterministic fake transport. Prove asynchronous establishment, concurrent acquisition, observations, verification, failure, recovery, expiry, cancellation, and stale callbacks. Do not add real network I/O.

## Add crates
- `observation-core`
- `verification-core`
- `transport-core`
- `egress-runtime`

## observation-core
Implement immutable `Observation`, `ObservationScope`, `Outcome`, `FailureAttribution`, `FailureCategory`, `FailureScope`, `Confidence`, and a health projection separate from raw evidence.

Rules:
- observation != interpretation;
- HTTP/application status cannot directly mark an endpoint/provider globally unhealthy;
- attribution references evidence IDs;
- unknown remains unknown;
- raw provider error strings never enter domain events.

## verification-core
Implement `VerifiedEgress`, `VerifiedProperties`, verifier contracts, and continuity integration.

Rules:
- provider declaration is not verification;
- missing evidence means unverified, not false;
- verification is generation-bound;
- stale verification cannot mutate current state.

## transport-core
Define transport-neutral asynchronous establishment/close contracts. Concrete transport errors are mapped to typed observations at the runtime boundary. Never expose credentials.

## FakeTransport
Implement deterministic scripted behavior: success, establishment failure, externally-controlled delay, and establish-then-fail. Tests must control ordering without sleeping.

## egress-runtime
Orchestrate:
1. acquire;
2. Establish/Reuse/Pending/rejection;
3. release mutation boundary;
4. async transport establishment;
5. record observations;
6. generation-safe state update;
7. optional verification;
8. record verification;
9. failure attribution;
10. recovery;
11. wake pending callers.

Never hold the manager mutation lock across `.await`.

## RouteLease
A lease identifies route ID/generation, identity ID/generation, and expiry. A lease is permission to use current routing state; it is not verification.

## Concurrency
For N compatible concurrent acquires while establishing:
- exactly one transport establishment;
- one establishment owner;
- others wait;
- all resolve from the same establishment result;
- no duplicate route allocation.

## Recovery
Implement only existing policies:
- `FailClosed`
- `RetrySameRoute`
- `ReplanPreservingIdentity`
- `ReplanAllowingIdentityChange`

Never add status-code-driven rotation.

## Time
Inject time. No sleeping in semantic tests. Tokio may exist in runtime/test crates only, never domain crates.

## Required tests
1. successful end-to-end acquire;
2. establishment observation recorded;
3. verification distinct from establishment;
4. verification failure does not rewrite establishment history;
5. 100 concurrent acquires cause one establishment;
6. pending callers wake on success;
7. pending callers wake on failure;
8. manager lock is not held during suspended transport;
9. stale success ignored after replacement;
10. stale failure ignored after replacement;
11. stale verification cannot mutate current identity;
12. route replacement can preserve identity;
13. identity-changing replacement increments identity generation;
14. unknown continuity never auto-promotes to preserved;
15. fail-closed never returns direct connectivity;
16. retry respects attempt limit;
17. expiry prevents new lease;
18. expiry behavior for existing lease matches documented policy;
19. destination HTTP 403 does not globally poison endpoint health;
20. provider authentication failure receives correct scope;
21. fake scripts are reproducible;
22. waiter cancellation does not corrupt state;
23. establishment-owner cancellation cannot leave permanent Establishing;
24. duplicate/stale callbacks are safely rejected/ignored;
25. shutdown resolves pending waiters.

## Property tests
Prove:
- arbitrary callback ordering cannot mutate newer generations;
- arbitrary observations cannot create verification;
- recovery cannot violate leakage policy;
- one route generation has at most one establishment owner;
- identity generation changes only for accepted continuity-breaking replacement.

## Typed runtime events
Add credential-free events for acquire, planning, establishment, verification, recovery, replacement, expiry, and lease issuance. Events are evidence/debugging aids, not authorization.

## Explicit exclusions
No real SOCKS5, HTTP CONNECT, MASQUE, QUIC, relay networking, DNS tunneling, provider APIs, geographic services, external verification endpoints, persistence, or adaptive/ML ranking.

## Quality gate
Run:
```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
All must pass.

## Definition of Done
A deterministic fake transport exercises:
`EgressRequest -> planning -> NetworkIdentity -> RoutePlan -> async establishment -> Observation -> Verification -> Lease -> Failure -> Attribution -> Recovery`

Concurrency, cancellation, expiry, replacement, and stale generations are tested.

Return the entire updated source tree and implementation report. STOP after M1.
