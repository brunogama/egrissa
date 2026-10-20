# Master Craft Instruction
Implement the complete Egress Runtime.

## Read first
`docs/PRD.md`, `docs/ARCHITECTURE.md`, `docs/DOMAIN-MODEL.md`, `docs/INVARIANTS.md`, `docs/SECURITY.md`, every `docs/adr/*.md`, `docs/TESTING.md`, then milestones M0→M7.

## Execution
Implement M0→M7 sequentially. Every milestone is a gate. Never weaken an earlier invariant to satisfy later code. If implementation reveals a real architectural conflict, write a proposed superseding ADR rather than hiding the decision.

Use Rust 2024. Domain crates remain transport-independent. Credentials remain outside domain objects. Inject time/randomness. Never hold manager mutation locks across network await. Observations are evidence, not conclusions. Verification requires evidence. Unknown remains unknown. Fail closed by default. Do not add destination-specific restriction-circumvention or status-code-driven identity rotation.

## Gate after every milestone
```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Final output
Return the entire repository plus `IMPLEMENTATION-REPORT.md` containing milestone status, invariant→test matrix, ADR deviations/new ADRs, limitations, security assumptions, build/test/run commands, and examples. Never claim unimplemented behavior.
