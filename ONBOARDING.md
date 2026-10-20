# Egrissa onboarding

This guide is evidence-based for the repository state inspected on October 19, 2026. Where repository documentation and current source disagree, this guide identifies the conflict instead of treating either as authoritative.

---

## Project overview

**Egrissa** is a Rust 2024 workspace for policy-driven network identity and egress orchestration. A workload provides an `EgressRequest` and `EgressPolicy`; the system plans an eligible route, establishes it at a transport boundary, collects immutable observations, attributes failures, verifies evidence, and returns a lease or an explicit recovery outcome. The central invariant is `desired != allocated != established != observed != verified`; see [`README.md`](README.md), [`docs/PRODUCT-GUIDE.md`](docs/PRODUCT-GUIDE.md), and [`AGENTS.md`](AGENTS.md).

The workspace uses resolver 3, Rust 1.85 MSRV, edition 2024, and `unsafe_code = "forbid"` ([`Cargo.toml`](Cargo.toml)). Runtime/test crates use Tokio; domain crates must not depend on Tokio, Serde, concrete RNGs, databases, or provider SDKs ([`AGENTS.md`](AGENTS.md)). Normal dependencies are deliberately sparse: `thiserror`, Tokio in allowed layers, plus protocol/TLS dependencies in the HTTP CONNECT adapter.

The primary developer workflow is library validation, not a hosted service: run formatting, Clippy, and the workspace tests. The only current binary is [`crates/egrissa-cli/src/main.rs`](crates/egrissa-cli/src/main.rs), which supports help/version and does not start a network service.

---

## Repository structure

| Path | Role |
| --- | --- |
| [`crates/egress-core`](crates/egress-core) | Requests, policies, capabilities, opaque IDs, and typed errors. |
| [`crates/identity-core`](crates/identity-core) | Logical identity, continuity, and lifecycle transitions. |
| [`crates/route-core`](crates/route-core) | Topology, planning, recovery, and deterministic route intelligence. |
| [`crates/observation-core`](crates/observation-core) | Immutable observations, attribution, and health projection. |
| [`crates/verification-core`](crates/verification-core) | Evidence-bound verifier contracts and verified egress values. |
| [`crates/transport-core`](crates/transport-core) | Transport-neutral establishment, tunnel, datagram, and packet contracts. |
| [`crates/egress-runtime`](crates/egress-runtime) | Tokio orchestration, lifecycle coordination, fake transport, and runtime integration tests. |
| [`crates/*-adapter`](crates) | SOCKS5, HTTP CONNECT, relay, MASQUE, and provider adapter boundaries and contract tests. |
| [`crates/egrissa-cli`](crates/egrissa-cli) | Current CLI distribution entry point. |
| [`tests`](tests) | Workspace integration and property tests; shared fixtures are in [`tests/support/mod.rs`](tests/support/mod.rs). |
| [`.github/workflows`](.github/workflows) | CI, PR policy, and release automation. |
| [`docs`](docs) | Product, brand, architecture, historical domain material, and ADRs. |
| [`prek.toml`](prek.toml) | Pre-commit formatting, linting, TOML, conflict, key, and environment-file checks. |

No database, HTTP API server, container definition, deployment manifest, migration system, package manager manifest, or infrastructure-as-code configuration was found in the tracked repository.

---

## Getting started

Follow [`QUICKSTART.md`](QUICKSTART.md) for the reproducible setup. The local quality gate is:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI installs Rust 1.85 plus `rustfmt` and `clippy` before running the same commands ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)). `cargo package --workspace --list` validates package contents without attempting registry resolution for unpublished workspace crates.

No secret, provider account, or local service is required for normal tests. `.env.local` is optional and ignored. Do not document or commit its values; [`.envrc`](.envrc) loads it only when present.

Production deployment instructions are **not present**. The release workflow produces downloadable binaries and GitHub release notes, but it does not publish crates to a registry ([`.github/workflows/release.yml`](.github/workflows/release.yml)).

---

## Key components

### Control and evidence flow

The intended flow is:

```text
EgressRequest + EgressPolicy
  -> NetworkIdentity + RoutePlan
  -> establishment
  -> Observation
  -> Attribution
  -> Verification
  -> RouteLease or Recovery
```

The three planes are documented in [`docs/PRODUCT-GUIDE.md`](docs/PRODUCT-GUIDE.md): control owns requirements, policy, identity, planning, and recovery; data owns adapters; evidence owns observations, attribution, health projection, and verification.

`EgressRuntime` is the public asynchronous orchestration boundary ([`crates/egress-runtime/src/lib.rs`](crates/egress-runtime/src/lib.rs)). It uses a generic `Transport` contract and test fake ([`crates/egress-runtime/src/fake.rs`](crates/egress-runtime/src/fake.rs)). Concrete byte-stream adapters implement `TunnelTransport`; MASQUE implements `DataPlaneTransport` ([`crates/transport-core/src/lib.rs`](crates/transport-core/src/lib.rs)). Runtime adapters and their integration coverage live in [`crates/egress-runtime/tests/transport_wrappers.rs`](crates/egress-runtime/tests/transport_wrappers.rs).

### State and safety boundaries

- `NetworkIdentity` is logical continuity, not an IP address ([`crates/identity-core/src/identity.rs`](crates/identity-core/src/identity.rs)).
- Route and identity generations are separate; stale feedback is rejected ([`tests/acquisition.rs`](tests/acquisition.rs)).
- `RoutePlan` is immutable desired routing; a route lease is permission to use current state, not verification.
- Domain objects use `CredentialRef`; adapter-local resolvers own secret material ([`docs/SECURITY.md`](docs/SECURITY.md), currently untracked in this checkout).
- The provider adapter materializes generic `InventorySnapshot` data rather than injecting provider policy into the planner ([`crates/reference-provider-adapter/src/lib.rs`](crates/reference-provider-adapter/src/lib.rs)).

Authentication, persistence, API endpoints, and a server-side authorization system are **not present** as tracked product surfaces. Relay forwarding authorization is adapter-local and explicitly allowlisted ([`crates/relay-adapter/src/lib.rs`](crates/relay-adapter/src/lib.rs)).

---

## Development workflow

### Branches, commits, and pull requests

`main` is the protected default branch. GitHub branch protection requires linear history, required status checks, pull-request flow, and resolved conversations. PR policy checks Conventional Commit subjects using these accepted types: `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, and `test` ([`.github/workflows/pr-policy.yml`](.github/workflows/pr-policy.yml)).

The PR policy workflow closes PRs whose author is not the repository owner. That is automated repository behavior, not a general GitHub platform-level prohibition on a user clicking “New pull request”.

### Tests and hooks

- Unit tests live beside crates and in crate `tests/` directories.
- Workspace integration and property tests live under [`tests`](tests).
- Adapter contract tests live under each adapter’s `tests/contract.rs`.
- [`prek.toml`](prek.toml) runs whitespace, TOML, merge-conflict, secret-key, `.env` syntax, formatting, and Clippy checks. Install/use of `prek` is not otherwise documented, so treat it as optional unless repository automation invokes it.

### Releases and changelog

On `main`, [`release.yml`](.github/workflows/release.yml) calculates semantic significance from Conventional Commits. A release-producing change must update the shared workspace version to the calculated SemVer value. The workflow runs formatting, lint, test, and package-content checks, creates an annotated tag and GitHub release notes, then uploads native CLI archives for Linux, macOS, and Windows. Generated GitHub release notes are the changelog mechanism; a tracked `CHANGELOG.md` is not present.

---

## Architecture decisions

Documented invariants in [`AGENTS.md`](AGENTS.md) and [`docs/INVARIANTS.md`](docs/INVARIANTS.md) include fail-closed direct-connect policy, explicit identity-changing recovery, bounded route lifetime, deterministic planning, no credentials in domain objects, and no control-plane mutation lock across `.await`.

Code-derived composition uses traits and adapters rather than concrete transport dependencies in domain crates. Time and selection are injected at runtime/planner seams. Errors are typed in core crates and adapter errors are intended to be sanitized before crossing boundaries; see [`crates/egress-core/src/error.rs`](crates/egress-core/src/error.rs) and [`docs/SECURITY.md`](docs/SECURITY.md).

Observability is evidence-first rather than metrics-service-first: observations, attributions, and verification records are domain values. No tracing exporter, metrics backend, database, or log-shipping configuration was found.

---

## Common tasks

### Add a route-planning feature

Add requirement/policy vocabulary in [`crates/egress-core/src`](crates/egress-core/src), enforce hard eligibility in [`crates/route-core/src/planner.rs`](crates/route-core/src/planner.rs), and add a behavior test in [`tests/planning.rs`](tests/planning.rs) or [`tests/properties.rs`](tests/properties.rs). Preserve the rule that unsupported topology fails before establishment.

### Add a transport adapter

Implement the transport-neutral contract in [`crates/transport-core/src/lib.rs`](crates/transport-core/src/lib.rs), keep addresses and credentials in adapter-local resolver traits, then add a local contract test like [`crates/socks5-adapter/tests/contract.rs`](crates/socks5-adapter/tests/contract.rs). Add runtime-wrapper coverage when the adapter must participate in acquisition.

### Write a runtime regression test

Use the public `EgressRuntime::acquire` seam and the fake/scripted transport in [`crates/egress-runtime/tests/m1_runtime.rs`](crates/egress-runtime/tests/m1_runtime.rs). Include generation, policy, and verification state in the assertion rather than testing private manager state.

### Debug a failed quality gate

Run the exact failed command first. If `cargo fmt` or `cargo clippy` is unavailable, install the Rust 1.85 `rustfmt` and `clippy` components as shown in [`QUICKSTART.md`](QUICKSTART.md). For package-content checks before publication, use `cargo package --workspace --list`; publish-style registry verification cannot resolve unpublished internal crates.

### Update a dependency

Edit the relevant root `[workspace.dependencies]` entry in [`Cargo.toml`](Cargo.toml), update all affected adapter/runtime code and tests, then run the full quality gate. Domain-crate dependency additions require justification against [`AGENTS.md`](AGENTS.md).

---

## Potential gotchas

1. **Documentation drift exists.** `AGENTS.md` says M2 has not started, while tracked commits and crate directories include M2 through M7. Likewise, [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) names planned crate names that do not match current `*-adapter` packages. Use `Cargo.toml` and current source as the execution inventory.
2. **Some historical references require reconciliation.** The committed README links only to tracked documentation, but `AGENTS.md` and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) should be checked against the workspace before making release claims.
3. **No production deployment exists.** No deployment/infrastructure files were found, and the product guide labels the project pre-release ([`README.md`](README.md)).
4. **Adapter maturity must be checked from current tests.** The product guide previously warned that concrete runtime integration needed verification; current tracked runtime wrapper and verification-safety tests should be read before making a release claim.
6. **Secrets are local only.** `.env.local` and `.envrc` are ignored, and `prek` validates `.env` syntax and detects private keys ([`.gitignore`](.gitignore), [`prek.toml`](prek.toml)). Never include values in examples or commits.

---

## Documentation and resources

- [`README.md`](README.md) - product synopsis, crate map, releases, and license.
- [`QUICKSTART.md`](QUICKSTART.md) - verified checkout instructions.
- [`docs/PRODUCT-GUIDE.md`](docs/PRODUCT-GUIDE.md) - product positioning and integration surfaces.
- [`docs/BRAND.md`](docs/BRAND.md) - working-name rationale and trademark-clearance limits.
- [`IMPLEMENTATION-REPORT.md`](IMPLEMENTATION-REPORT.md) - M7 implementation report and test matrix.
- [`docs/adr`](docs/adr) - ADRs 0021 through 0026 are tracked; ADRs 0001 through 0020 are currently untracked.
- [`CRAFT.md`](CRAFT.md) and [`CRAFT-M1.md`](CRAFT-M1.md) - milestone handoff material. Their status should be cross-checked against current code and commits.
- [`.github/workflows`](.github/workflows) and [`prek.toml`](prek.toml) - enforcement and automation.

A tracked `CONTRIBUTING.md`, API schema, deployment guide, database schema, operations runbook, and package publication guide are not present.

---

## First-week checklist

1. Install Rust 1.85, `rustfmt`, and `clippy`; run [`QUICKSTART.md`](QUICKSTART.md).
2. Run `cargo run --package egrissa-cli -- --help` to confirm the current public executable surface.
3. Read [`AGENTS.md`](AGENTS.md), [`README.md`](README.md), [`docs/PRODUCT-GUIDE.md`](docs/PRODUCT-GUIDE.md), and the current runtime tests.
4. Walk the fake acquisition flow in [`crates/egress-runtime/tests/m1_runtime.rs`](crates/egress-runtime/tests/m1_runtime.rs), then map it to `EgressRuntime` in [`crates/egress-runtime/src/lib.rs`](crates/egress-runtime/src/lib.rs).
5. Make a safe documentation or focused test change on a branch from `main` with a Conventional Commit subject.
6. Run focused tests for the changed crate, then the full quality gate.
7. Open an owner-authored PR, wait for `Verify workspace` and `Conventional commits`, resolve conversations, and merge linearly.
