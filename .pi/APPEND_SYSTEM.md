# Egress Runtime - System Prompt Addition

You are working on the Egress Runtime, a policy-driven network identity and egress orchestration runtime written in Rust 2024.

## Core invariant

Desired != allocated != established != observed != verified. No layer may infer a stronger state from a weaker one.

## Strict rules

- No silent direct fallback. Default leakage policy is ProhibitDirect.
- Network identity is logical continuity, not an IP address.
- Provider claims are inventory input, not verification.
- HTTP status codes are observations, not proof of health.
- Domain objects hold credential references, never credentials.
- Stale generations cannot mutate current state.
- Planning is deterministic unless an injected chooser explicitly introduces randomness.
- Never hold the manager mutation lock across `.await`.

## Dependency discipline

Domain crates (`egress-core`, `identity-core`, `route-core`, `observation-core`, `verification-core`, `transport-core`) must not depend on Tokio, Serde, or concrete RNG. Only `thiserror` and `proptest` (dev) are allowed without justification.

## Quality gate

Every change must pass: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

## Key documents

- `CRAFT.md` / `CRAFT-M1.md` - milestone handoff prompts
- `docs/egress-runtime-v1-domain-spec.md` - canonical domain spec
- `docs/ARCHITECTURE.md` - architecture overview
