# Egrissa

**Policy-driven network identity and egress orchestration for Rust.**

Egrissa lets workloads declare the egress and identity properties they require instead of choosing a proxy or handling credentials. It plans an eligible route, establishes it through a transport boundary, records immutable evidence, verifies only what evidence supports, and applies explicit recovery without silently weakening policy.

```text
Desired != allocated != established != observed != verified
```

> [!WARNING]
> Egrissa is pre-release software. It has a complete semantic kernel, deterministic fake runtime, and adapter contract suites, but it is not yet a production-ready managed egress service. See [Current maturity](#current-maturity) before adopting it.

---

## Why Egrissa

Network identity is a logical continuity contract, not an IP address. Egrissa keeps requirements, allocation, route establishment, observations, attribution, and verification separate so a weaker fact cannot be reported as a stronger one.

It is intended for automation platforms, integration infrastructure, controlled crawlers, CI and network tests, privacy-sensitive clients, distributed agents, and geo or network behavior tests.

Egrissa is not an anonymity product, an IP-rotation service, or a tool for circumventing destination access controls.

---

## Guarantees

- **Fail closed.** `ProhibitDirect` is the default. Unsupported or ineligible routes fail before establishment rather than falling back to direct connectivity.
- **Logical identities.** Identities have opaque IDs, lifetimes, scope, and independent generations. Continuity is explicit: `Preserved`, `Changed`, or `Unknown`.
- **Evidence discipline.** Provider claims are inventory input, observations are not verification, and destination evidence cannot globally poison endpoint health.
- **Safe concurrency.** One establishment owns each route generation, stale callbacks cannot mutate current state, and manager mutation locks are not held across network awaits.
- **Credential boundaries.** Domain objects hold `CredentialRef` values, never credential material.
- **Deterministic planning.** Planning and default route selection are deterministic. Bounded adaptive selection requires an explicit injected chooser.
- **Explainable decisions.** Route intelligence records eligible candidates, exclusions, scoring inputs, selected route, and any deterministic fallback.

---

## Workspace

| Crate | Responsibility |
| --- | --- |
| `egress-core` | Requirements, policy, capabilities, IDs, and typed errors |
| `identity-core` | Logical identity and continuity lifecycle |
| `route-core` | Eligibility planning, topology, recovery, and deterministic route intelligence |
| `observation-core` | Immutable observations, attribution, and health projection |
| `verification-core` | Evidence-bound verification contracts |
| `transport-core` | Transport-neutral establishment, tunnel, datagram, and packet contracts |
| `egress-runtime` | Asynchronous orchestration and deterministic fake transport |
| `socks5-adapter` | SOCKS5 TCP CONNECT adapter contract |
| `http-connect-adapter` | HTTP CONNECT adapter contract, including TLS-to-proxy configuration |
| `relay-adapter` | CONNECT relay client/server adapter contract with explicit forwarding policy |
| `masque-adapter` | CONNECT-UDP and CONNECT-IP adapter contract with typed negotiation |
| `provider-core` | Generic provider binding lifecycle and inventory publication |
| `reference-provider-adapter` | Configurable generic provider-to-inventory adapter |

---

## Getting started

### Prerequisites

- Rust 1.85 or later
- Cargo

### Build and test

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The repository uses Rust 2024 edition and resolver 3. Tests cover domain units, properties, deterministic fake-runtime acquisition, adapter contracts, lifecycle transitions, stale callbacks, recovery, and route intelligence.

---

## Documentation

- [Product guide](docs/PRODUCT-GUIDE.md) - product model, supported surfaces, security guarantees, and maturity limits.
- [Brand guide](docs/BRAND.md) - commercial naming rationale and clearance steps.
- [Architecture](docs/ARCHITECTURE.md) - control, data, and evidence planes.
- [Implementation report](IMPLEMENTATION-REPORT.md) - milestone-level implementation matrix.

---

## Current maturity

The repository implements M0 through M7 semantic, planning, adapter-contract, provider, and route-intelligence milestones. It remains pre-release software: use this table to keep public claims aligned with repository evidence.

| Area | Status | Evidence |
| --- | --- | --- |
| Semantic/runtime milestones | Implemented through M7 | [Implementation report](IMPLEMENTATION-REPORT.md), workspace crates, and tests |
| Concrete adapter runtime integration | Present, but verify the relevant integration path before making a production claim | [`crates/egress-runtime/tests/transport_wrappers.rs`](crates/egress-runtime/tests/transport_wrappers.rs) |
| Verification safety | Required-verification reuse and continuity-evidence regressions are covered | [`crates/egress-runtime/tests/verification_safety.rs`](crates/egress-runtime/tests/verification_safety.rs) |
| CLI | Help/version distribution entry point only | [`crates/egrissa-cli/src/main.rs`](crates/egrissa-cli/src/main.rs) |
| Deployment or hosted service | Not present in the tracked repository | No tracked deployment or infrastructure configuration |
| Documentation baseline | Requires reconciliation before release claims | [`AGENTS.md`](AGENTS.md) milestone status lags the current workspace |

Read the documentation as an engineering contract, not as a promise of a hosted service or anonymity guarantee.

---

## Releases and packages

A push to `main` calculates release significance from Conventional Commits. A release-producing change must update the shared workspace version to the calculated SemVer value. The workflow then tags that merged content, creates GitHub release notes from merged pull requests, and uploads native `egrissa` binaries for macOS, Windows, and Linux. It never publishes crates to a registry.

Source packages retain a shared version and MSRV (`1.85`). Registry publishing, if adopted, must add explicit repository, homepage, documentation, and publishing-ownership metadata before crates are released.

---

## License

All source packages in this workspace are licensed under the [MIT License](LICENSE).
