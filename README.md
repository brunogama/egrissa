# Egress Runtime

Policy-driven network identity and egress orchestration runtime.

## Current status

---

- M0 semantic kernel: complete.
- M1 fake end-to-end runtime: complete.
- No real network I/O. M2 introduces the SOCKS5 reference adapter.

## Workspace

---

```text
crates/egress-core/        # Requirements, policy, capabilities, IDs, errors
crates/identity-core/      # Network identity, continuity, lifecycle
crates/route-core/         # Route planning, topology, lifecycle
crates/observation-core/   # Immutable observations and health projection
crates/verification-core/  # Evidence-bound verification contracts
crates/transport-core/     # Transport-neutral async contracts
crates/egress-runtime/     # Async orchestration and deterministic fake transport
tests/                     # Integration and property tests
```

## Validate

---

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Documents

---

- `CRAFT.md` records the completed M0 handoff.
- `CRAFT-M1.md` records the completed M1 requirements.
- `docs/egress-runtime-v1-domain-spec.md` defines the domain model.
- `docs/ARCHITECTURE.md` summarizes the architecture.
- `docs/M0.md` defines the M0 milestone.
- `docs/ROADMAP.md` lists milestones M0 through M7.
