# Egress Runtime

Policy-driven network identity and egress orchestration runtime.

M0 implements the semantic kernel as a Rust 2024 workspace. It performs no network I/O.
It does not include transport adapters, provider APIs, production verification, or an async runtime.

## Workspace

```text
.
├── crates/
│   ├── egress-core/    # Requirements, policy, capabilities, IDs, errors, evidence contracts
│   ├── identity-core/  # Logical identity, continuity, lifecycle, verification output contract
│   └── route-core/     # Explicit topologies, deterministic planning, synchronous acquisition
└── tests/             # Cross-crate acceptance and property tests
```

Dependencies are limited to `thiserror` and the test-only `proptest`.
The library crates use the standard library for time and collections.

---

## Validate

Requires Rust 1.85 or newer.

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The root `tests/` targets are registered in `crates/route-core/Cargo.toml`, so
`cargo test --workspace` runs them despite the root being a virtual workspace.

---

## Public boundaries

- `egress_core::{request, policy, capability, ids, error, evidence}` define transport-independent values.
- `identity_core::identity::NetworkIdentity` allocates a logical identity, validates lifecycle transitions, and explicitly accepts replacements. It has no IP address field.
- `route_core::planner::RoutePlanner` takes a `PlanContext`, including an allocated identity, inventory, attributed health, caller-selected IDs/generations, and explicit time. `CandidateChooser` is the only selection seam.
- `route_core::lifecycle::RouteManager` returns `Establish`, `Pending`, `Reuse`, or a typed rejection. The host serializes mutations and establishes only after releasing its lock. Completions carry a full `FeedbackKey`.
- `RouteManager::recover` returns an explicit identity-preserving or identity-changing decision. It never infers continuity from transport success or HTTP status.

Defaults prohibit direct leakage and fail closed on recovery. M0 has no direct topology,
even with `AllowDirect`. `RequireBeforeUse` returns `VerificationUnavailable` until a
verification implementation exists.

---

## Documents

- [CRAFT.md](CRAFT.md): implementation instructions and exit gate
- [Domain specification](docs/egress-runtime-v1-domain-spec.md): complete v1 design
- [Architecture](docs/ARCHITECTURE.md): architecture summary
- [M0](docs/M0.md): semantic-kernel scope
- [Implementation report](docs/M0-IMPLEMENTATION.md): invariants, test coverage, design choices, and open questions
