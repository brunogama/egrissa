# Craft: Egress Runtime M0

Implement the Egress Runtime M0 semantic kernel from the documents in this package.

Read these first:
1. `docs/egress-runtime-v1-domain-spec.md`
2. `docs/ARCHITECTURE.md`
3. `docs/M0.md`

## Objective

Create a compiling Rust 2024 workspace named `egress-runtime`.

Implement only the semantic/control-plane kernel. Do not implement real networking yet.

## Required workspace

```text
egress-runtime/
├── Cargo.toml
├── README.md
├── crates/
│   ├── egress-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ids.rs
│   │       ├── request.rs
│   │       ├── policy.rs
│   │       ├── capability.rs
│   │       └── error.rs
│   ├── identity-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── identity.rs
│   │       ├── continuity.rs
│   │       └── lifecycle.rs
│   └── route-core/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── topology.rs
│           ├── route.rs
│           ├── planner.rs
│           └── lifecycle.rs
└── tests/
```

## Dependencies

Keep the kernel small.

Allowed initially:
- `thiserror = "2"`
- `proptest = "1"` as a dev dependency

Do not add Tokio, reqwest, hyper, QUIC, SOCKS libraries, provider SDKs, Serde, databases, or concrete RNG dependencies unless the specification cannot be implemented without them.

Pass time and randomness explicitly.

## Canonical objects

Implement the semantics for:
- `EgressRequest`
- `NetworkIdentity`
- `EgressPolicy`
- `RoutePlan`

Create interfaces/placeholders needed for the later:
- `Observation`
- `FailureAttribution`
- `VerifiedEgress`

Do not collapse these concepts.

## Non-negotiable invariants

1. Desired != allocated != established != observed != verified.
2. Network identity is not an IP address.
3. Unknown identity continuity remains unknown.
4. Route generation and identity generation are independent.
5. Stale generations cannot mutate current state.
6. No silent direct fallback under `ProhibitDirect`.
7. HTTP/application status codes do not directly mutate route health.
8. Provider claims are not verification.
9. Unsupported topology is rejected before establishment.
10. Identity-changing recovery is explicit.
11. Route expiry cannot exceed dependency validity.
12. Exactly one establishment exists per route generation.
13. Domain objects contain credential references, never credentials.
14. Planning is deterministic unless an injected chooser explicitly introduces randomness.

## Tests

Use ordinary unit tests plus `proptest`.

At minimum prove:
- repeated compatible acquisition reuses the route/identity;
- concurrent acquisition semantics permit one establishment only;
- `now == expires_at` is expired;
- no eligible candidate produces a typed error;
- `ProhibitDirect` cannot produce direct fallback;
- stale route feedback cannot mutate current route state;
- stale identity feedback cannot mutate current identity state;
- distinct-operator policy cannot produce same-operator combinations;
- unsupported transport/topology cannot be planned;
- route lifetime is bounded by all dependencies;
- identity replacement cannot claim `Preserved`;
- deterministic inputs produce deterministic planning results;
- injected chooser cannot return an out-of-range candidate without a typed error.

## Architectural rule

Do not distort the domain model to make a transport implementation convenient.

M0 must contain zero real network I/O.

## Definition of done

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All must pass.

Return the complete source tree, not snippets. Include a short implementation report listing:
- implemented invariants;
- tests;
- deliberate deviations from the specification;
- unresolved design questions.

Do not proceed to SOCKS5, HTTP CONNECT, MASQUE, relay networking, or provider adapters. Stop when M0 is complete.
