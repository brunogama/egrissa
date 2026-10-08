# Egress Runtime Agent Instructions

## Project identity

---

Policy-driven network identity and egress orchestration runtime. Rust 2024 workspace, edition 2024, resolver 3.

## Read first

---

- `CRAFT.md` records the completed M0 handoff.
- `CRAFT-M1.md` records the completed M1 requirements.
- `docs/egress-runtime-v1-domain-spec.md` defines the domain model.
- `docs/ARCHITECTURE.md` summarizes the architecture.
- `docs/ROADMAP.md` lists milestones M0 through M7.

## Current status

---

- M0 semantic kernel: complete.
- M1 fake end-to-end runtime: complete.
- M2 SOCKS5 reference adapter: not started.
- No real network I/O exists before M2.

## Workspace

---

```text
crates/egress-core/        # Requirements, policy, capabilities, IDs, errors
crates/identity-core/      # Network identity, continuity, lifecycle
crates/route-core/         # Route planning, topology, lifecycle
crates/observation-core/   # Immutable observations and health projection
crates/verification-core/  # Evidence-bound verification contracts
crates/transport-core/     # Transport-neutral asynchronous contracts
crates/egress-runtime/     # Async orchestration and deterministic fake transport
tests/                     # Integration and property tests
```

## Non-negotiable invariants

---

1. Desired != allocated != established != observed != verified.
2. Network identity is not an IP address.
3. Unknown identity continuity remains unknown.
4. Route generation and identity generation are independent.
5. Stale generations cannot mutate current state.
6. No silent direct fallback under `ProhibitDirect`.
7. HTTP and application status codes do not directly mutate route health.
8. Provider claims are not verification.
9. Unsupported topology is rejected before establishment.
10. Identity-changing recovery is explicit.
11. Route expiry cannot exceed dependency validity.
12. Exactly one establishment occurs per route generation.
13. Domain objects contain credential references, never credentials.
14. Planning is deterministic unless an injected chooser introduces randomness.

## Rules

---

- Do not distort the domain model to make transport implementation convenient.
- Do not add real network I/O before M2.
- Keep Tokio, Serde, concrete RNGs, databases, and provider SDKs out of domain crates.
- Use Tokio only in runtime and test crates.
- Pass time and randomness explicitly.
- Never hold the manager mutation lock across `.await`.
- Keep `unsafe_code = "forbid"` in workspace lints.

## Quality gate

---

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run all three checks before declaring a milestone complete.

## Allowed dependencies

---

- `thiserror = "2"`.
- `proptest = "1"` for development dependencies only.
- Tokio is allowed in runtime and test crates, never in domain crates.

Justify any additional dependency against the domain specification.
