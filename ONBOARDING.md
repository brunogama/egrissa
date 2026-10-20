# Egrissa onboarding

This guide describes the tracked source and automation visible in this checkout. It distinguishes source-derived facts from repository documentation where they conflict. It does not claim a managed deployment, a production service, or external integrations that are not represented in the repository.

## 1. Project overview

**Egrissa** is a Rust workspace for policy-driven network identity and egress orchestration. A caller supplies an `EgressRequest`, `EgressPolicy`, identity, inventory, observations, and time; the runtime plans and establishes an eligible route, records observations, optionally verifies egress evidence, and returns a lease or typed error. The central semantic rule is:

```text
desired != allocated != established != observed != verified
```

The current public executable is the `egrissa` CLI. It prints help or the package version; it is not a proxy daemon or a complete workload-facing network client ([`crates/egrissa-cli/src/main.rs`](crates/egrissa-cli/src/main.rs)). The primary developer workflow is therefore building and validating the Rust workspace, not deploying a service.

| Concern | Evidence-backed state |
| --- | --- |
| Language and edition | Rust, edition 2024 ([`Cargo.toml`](Cargo.toml)). |
| Minimum supported Rust | `rust-version = "1.85"`; CI installs Rust 1.85.0. No tracked `rust-toolchain.toml` selects it locally. |
| Workspace | 14 Cargo packages, resolver 3, shared version `0.1.0`, and MIT license metadata ([`Cargo.toml`](Cargo.toml)). |
| Build and test tools | Cargo, `rustfmt`, Clippy, Tokio tests, and GitHub Actions ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)). |
| Runtime dependencies | `thiserror` in the core layer; Tokio in the runtime and socket adapters; the HTTP CONNECT adapter also uses Rustls, Tokio-Rustls, and native certificate loading ([`crates/http-connect-adapter/Cargo.toml`](crates/http-connect-adapter/Cargo.toml)). |
| Development dependencies | `proptest` for selected property tests and Tokio features for runtime/adapter tests. |
| Compatibility constraints | Workspace linting forbids `unsafe`; domain crates must remain independent of Tokio, Serde, concrete RNGs, databases, and provider SDKs ([`AGENTS.md`](AGENTS.md)). |

## 2. Repository structure

| Path | Purpose |
| --- | --- |
| [`crates/egress-core`](crates/egress-core) | Egress request/policy vocabulary, capabilities, opaque identifiers, and typed errors. |
| [`crates/identity-core`](crates/identity-core) | Network identity, continuity, and identity lifecycle types. |
| [`crates/route-core`](crates/route-core) | Route topology, planning, lifecycle, recovery, and deterministic route intelligence. |
| [`crates/observation-core`](crates/observation-core) | Immutable observations, attribution, and health projection. |
| [`crates/verification-core`](crates/verification-core) | Evidence-bound verification contracts and verified-egress values. |
| [`crates/transport-core`](crates/transport-core) | Transport-neutral establishment, tunnel, datagram, packet, and close contracts. |
| [`crates/egress-runtime`](crates/egress-runtime) | Tokio orchestration, adapter wrappers, deterministic fake transport, and runtime integration tests. |
| [`crates/socks5-adapter`](crates/socks5-adapter) | SOCKS5 TCP CONNECT adapter and local contract tests. |
| [`crates/http-connect-adapter`](crates/http-connect-adapter) | HTTP CONNECT adapter, optional TLS-to-proxy support, and local contract tests. |
| [`crates/relay-adapter`](crates/relay-adapter) | Relay client/server boundary with explicit forwarding policy. |
| [`crates/masque-adapter`](crates/masque-adapter) | Typed CONNECT-UDP and CONNECT-IP adapter boundary. |
| [`crates/provider-core`](crates/provider-core) and [`crates/reference-provider-adapter`](crates/reference-provider-adapter) | Provider binding lifecycle plus a configurable inventory adapter. |
| [`crates/egrissa-cli`](crates/egrissa-cli) | The `egrissa` executable entry point. |
| [`tests`](tests) | Cross-crate semantic, planning, acquisition, recovery, and route-intelligence tests. |
| [`crates/*/tests`](crates) | Crate-level lifecycle, verification, adapter contract, and runtime-wrapper tests. |
| [`.github/workflows`](.github/workflows) | CI, pull-request policy, and release automation. |
| [`docs`](docs) | Architecture, product, brand, milestone, domain-specification, roadmap, and ADR material. |
| [`prek.toml`](prek.toml) | Optional pre-commit hook configuration. |

No tracked HTTP server, API schema, database schema, migrations, container definition, infrastructure-as-code, deployment manifest, or package-manager manifest besides Cargo was found.

## 3. Getting started

Start with [`QUICKSTART.md`](QUICKSTART.md). The same commands reflected by CI are:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo package --workspace --list
```

Build the distributable binary with:

```sh
cargo build --release --package egrissa-cli
```

The release workflow packages this binary as Linux, macOS, and Windows archives after it creates a release; it is automation, not a local deployment procedure ([`.github/workflows/release.yml`](.github/workflows/release.yml)).

### Configuration, secrets, and external requirements

Normal builds and tests require no environment variables, provider account, database, network service, or manually started local daemon. The repository ignores `.env.local` and `.envrc` ([`.gitignore`](.gitignore)); no tracked loader or variable contract is present. Treat any locally created environment file as private and do not add its values to documentation or commits.

### Evidence-based troubleshooting

- **`cargo fmt` or Clippy is unavailable:** install the Rust 1.85.0 `rustfmt` and `clippy` components shown in [`QUICKSTART.md`](QUICKSTART.md). CI uses a minimal toolchain and explicitly adds them.
- **Publish-style package verification fails to resolve an internal crate:** use the CI-equivalent `cargo package --workspace --list`. Internal workspace crates are not published to crates.io, so registry-backed packaging is intentionally not part of CI.
- **A design document names a missing path or obsolete milestone state:** see [Potential gotchas](#8-potential-gotchas) before treating it as current implementation truth.

Production deployment instructions, operational runbooks, and a managed-service configuration are **not present**.

## 4. Key components

### Public entry points

- **CLI:** `crates/egrissa-cli/src/main.rs` dispatches no argument, `--help`, `-h`, and `help` to usage output; `--version`, `-V`, and `version` print the package version. Unknown arguments write a diagnostic to stderr and exit with status 2.
- **Runtime library:** `EgressRuntime<T, C>` in [`crates/egress-runtime/src/lib.rs`](crates/egress-runtime/src/lib.rs) is the asynchronous orchestration boundary. It composes a `Transport`, injected `Clock`, and optional `EgressVerifier`; callers drive it through `acquire`, state/event accessors, observations, and async `shutdown`.
- **Transport boundary:** [`crates/transport-core/src/lib.rs`](crates/transport-core/src/lib.rs) defines `Transport`, `TunnelTransport`, `DataPlaneTransport`, `Tunnel`, `DatagramTunnel`, and `PacketTunnel`. `TunnelTransportAdapter` and `DataPlaneTransportAdapter` in the runtime retain and close adapter-owned tunnels.

### State and data model

The intended state flow is:

```text
EgressRequest + EgressPolicy
  -> NetworkIdentity + RoutePlan
  -> establishment
  -> immutable observation and attribution
  -> optional verification
  -> RouteLease or explicit recovery/error
```

A lease is not verification. Identity continuity is logical rather than IP-based; an IP observation alone must not prove continuity. Route and identity generations are independent, and stale feedback must not mutate current state. These are documented constraints in [`AGENTS.md`](AGENTS.md) and source-level types span `identity-core`, `route-core`, `observation-core`, and `verification-core`.

### Authentication, persistence, and integrations

There is no tracked HTTP API, server-side authentication system, persistence layer, database, or external provider SDK. Domain values contain opaque credential references, while address and credential resolution stay at adapter boundaries. This division is documented in [ADR 0021](docs/adr/0021.md) and [ADR 0022](docs/adr/0022.md).

SOCKS5, HTTP CONNECT, relay, MASQUE, and reference-provider crates are adapter surfaces. They are not evidence that the repository provides a hosted provider, relay deployment, or anonymous browsing service.

## 5. Development workflow

### Branches, commits, and pull requests

The repository policy workflow accepts Conventional Commit subjects beginning with `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, or `test` ([`.github/workflows/pr-policy.yml`](.github/workflows/pr-policy.yml)). It also closes pull requests from accounts other than the repository owner. This is repository workflow behavior, not a general GitHub platform restriction.

The tracked files do not prove live GitHub branch-protection settings, reviewer requirements, or merge permissions; inspect the repository settings when those details matter.

### Tests, linting, and hooks

- Core semantic/property tests are wired through [`crates/route-core/Cargo.toml`](crates/route-core/Cargo.toml) into root [`tests`](tests).
- Runtime tests are under [`crates/egress-runtime/tests`](crates/egress-runtime/tests); adapter contract tests use each adapter's `tests/contract.rs`.
- CI runs formatting, strict Clippy, all workspace tests, license metadata validation, and package-content listing ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)).
- [`prek.toml`](prek.toml) configures whitespace, TOML, merge-conflict, private-key, environment-file syntax, format, and Clippy hooks. It does not establish that `prek` is installed or required locally; install/use instructions are not tracked.

### Releases and changelog

On pushes to `main`, [`release.yml`](.github/workflows/release.yml) examines commits since the latest `v*` tag. Breaking conventional commits request a major version, `feat` requests a minor version, and `fix`/`perf` request a patch version. A requested release fails if the shared workspace version does not match the calculated version. The workflow tags the result, creates GitHub-generated notes, and builds CLI archives for Linux, macOS, and Windows.

No tracked `CHANGELOG.md` or registry-publishing workflow exists.

## 6. Architecture decisions

### Documented decisions

The most useful current ADRs are:

- [ADR 0021](docs/adr/0021.md): data-plane tunnels are adapter-owned and transport-core stays Tokio-neutral.
- [ADR 0022](docs/adr/0022.md): HTTP CONNECT resolves addresses and credentials at its adapter boundary; TLS-to-proxy is optional and validated there.
- [ADR 0023](docs/adr/0023.md): relay forwarding requires an explicit allowlist and does not fall back to direct traffic.
- [ADR 0024](docs/adr/0024.md): MASQUE keeps UDP datagrams and IP packets as typed data planes rather than byte streams.
- [ADR 0025](docs/adr/0025.md): providers project immutable binding and observation facts into generic planner input; provider assertions are not verification.
- [ADR 0026](docs/adr/0026.md): route intelligence ranks immutable, scoped facts without allowing destination evidence to globally poison endpoint health.

### Code-derived composition and error handling

Domain crates depend on typed data and traits rather than concrete transports. Runtime composition injects time through `Clock`, transport through `Transport`, and verification through `EgressVerifier`. `TransportOutcome::Failed` means establishment failed before an adapter retained a tunnel; `EstablishedThenFailed` preserves the distinct state where a tunnel existed before terminal failure ([`crates/transport-core/src/lib.rs`](crates/transport-core/src/lib.rs)).

`EgressRuntime` removes tunnels from its retained map before asynchronously closing them, so the control mutex is not held across that await. Observations and verification values are domain evidence; no tracing backend, metrics exporter, or log-shipping configuration is tracked.

## 7. Common tasks

### Add a planning requirement

1. Add request/policy or capability vocabulary under [`crates/egress-core/src`](crates/egress-core/src).
2. Enforce hard eligibility in [`crates/route-core/src`](crates/route-core/src), not in a transport adapter.
3. Add a root semantic/planning/property test under [`tests`](tests) or a focused route-core test.
4. Run the full quality gate. Preserve fail-closed behavior and deterministic planning unless an injected chooser is the explicit design seam.

### Add a transport adapter

1. Implement the appropriate `TunnelTransport` or `DataPlaneTransport` contract in [`crates/transport-core/src/lib.rs`](crates/transport-core/src/lib.rs).
2. Keep concrete endpoints, raw credentials, and protocol responses inside the adapter crate.
3. Add a local adapter contract test, following [`crates/socks5-adapter/tests/contract.rs`](crates/socks5-adapter/tests/contract.rs) or the closest topology.
4. Add runtime-wrapper coverage in [`crates/egress-runtime/tests/transport_wrappers.rs`](crates/egress-runtime/tests/transport_wrappers.rs) when runtime ownership/closure behavior changes.

### Add a runtime regression test

Use `EgressRuntime::acquire` with the deterministic fake transport in [`crates/egress-runtime/src/fake.rs`](crates/egress-runtime/src/fake.rs). Existing runtime scenarios are in [`crates/egress-runtime/tests/m1_runtime.rs`](crates/egress-runtime/tests/m1_runtime.rs), [`verification_safety.rs`](crates/egress-runtime/tests/verification_safety.rs), and [`transport_wrappers.rs`](crates/egress-runtime/tests/transport_wrappers.rs). Assert observable generation, verification, observation, event, or closure behavior rather than private state.

### Diagnose a failed CI job

Re-run the failing command exactly. For a local reproduction of the CI sequence:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo package --workspace --list
```

### Update a dependency

Update the relevant workspace dependency in [`Cargo.toml`](Cargo.toml), or the adapter-local HTTP/TLS dependency in [`crates/http-connect-adapter/Cargo.toml`](crates/http-connect-adapter/Cargo.toml). Read every affected package manifest and test surface, then run the full quality gate. Adding a dependency to a domain crate needs an explicit design justification because of the constraints in [`AGENTS.md`](AGENTS.md).

## 8. Potential gotchas

1. **Some documentation is stale.** [`AGENTS.md`](AGENTS.md) says M2 has not started and that no real network I/O exists before M2, while the workspace contains SOCKS5, HTTP CONNECT, relay, MASQUE, provider, and CLI packages. Treat current manifests, source, and tests as the execution inventory.
2. **Some tracked references are locally absent in this working tree.** `CRAFT.md` references `docs/PRD.md`, `docs/DOMAIN-MODEL.md`, `docs/INVARIANTS.md`, `docs/SECURITY.md`, and `docs/TESTING.md`; the project tree inspected for this guide does not contain those paths. They must not be treated as current local instructions without restoration or verification.
3. **The architecture document has an outdated crate list.** [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) names `transport-socks5`, `transport-http-connect`, and other target names that differ from the actual `*-adapter` workspace members in [`Cargo.toml`](Cargo.toml).
4. **A clean package listing is not proof of publishability.** CI uses `cargo package --workspace --list`, because unpublished path dependencies cannot pass crates.io resolution. Do not substitute it for a release-to-registry check.
5. **The CLI is intentionally small.** `egrissa` has no route configuration, proxy/server startup, or connection command; its current surface is help/version only ([`crates/egrissa-cli/src/main.rs`](crates/egrissa-cli/src/main.rs)).
6. **Secret files are deliberately ignored.** [`.gitignore`](.gitignore) excludes `.env.local` and `.envrc`; [`.github/workflows/ci.yml`](.github/workflows/ci.yml) does not require them. Do not add secret examples or real values to tests and docs.
7. **No automatic background expiry scheduler is documented.** Route lifecycle/expiry behavior is tested through the runtime and route manager; adding a background reaper would be a new lifecycle design, not routine maintenance.

## 9. Documentation and resources

| Resource | Use |
| --- | --- |
| [`README.md`](README.md) | Public project summary, crate map, maturity language, releases, and license. |
| [`QUICKSTART.md`](QUICKSTART.md) | Minimal checkout/build/quality-gate instructions. |
| [`docs/PRODUCT-GUIDE.md`](docs/PRODUCT-GUIDE.md) | Product positioning and conceptual three-plane model. It links to some locally absent documents; verify those links before relying on them. |
| [`docs/BRAND.md`](docs/BRAND.md) | Working-name guidance and the explicitly incomplete trademark-clearance status. |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Short architecture sketch; cross-check its crate list with `Cargo.toml`. |
| [`docs/adr`](docs/adr) | Architecture decisions; ADRs 0021–0026 cover current adapter, provider, and route-intelligence work. |
| `docs/egress-runtime-v1-domain-spec.md`, `docs/ROADMAP.md`, `docs/M0.md` | Tracked historical/domain material, but locally absent during this review; restore or verify them before use. |
| [`CRAFT.md`](CRAFT.md), [`CRAFT-M1.md`](CRAFT-M1.md), [`IMPLEMENTATION-REPORT.md`](IMPLEMENTATION-REPORT.md) | Milestone instructions and implementation report; read alongside current code because older status statements may be stale. |
| [`.github/workflows`](.github/workflows), [`prek.toml`](prek.toml) | Enforced/automated checks and optional local hooks. |

A tracked `CONTRIBUTING.md`, API specification, security runbook, database schema, migration guide, deployment guide, operations runbook, and package-publication guide were not found.

## 10. First-week checklist

1. Install Rust 1.85, `rustfmt`, and Clippy; complete [`QUICKSTART.md`](QUICKSTART.md).
2. Run `cargo run --package egrissa-cli -- --help` to see the only current executable surface.
3. Read [`AGENTS.md`](AGENTS.md), [`docs/PRODUCT-GUIDE.md`](docs/PRODUCT-GUIDE.md), the ADRs, and this guide's gotchas before changing a domain boundary.
4. Walk `EgressRuntime::acquire` in [`crates/egress-runtime/src/lib.rs`](crates/egress-runtime/src/lib.rs) with a test in [`crates/egress-runtime/tests/m1_runtime.rs`](crates/egress-runtime/tests/m1_runtime.rs).
5. Make a safe first contribution: a focused test, an adapter-contract assertion, or a documentation correction backed by current source.
6. Run the changed crate's focused tests, then the full workspace quality gate.
7. Use a Conventional Commit subject when opening a pull request and confirm the repository's current GitHub settings before relying on assumptions about review or merge rules.
