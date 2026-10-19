# Egrissa Runtime Product Guide

Egrissa is a policy-driven runtime for selecting and governing network egress. A workload states the egress and identity properties it requires. Egrissa evaluates eligible routes, establishes a route through an adapter boundary, records evidence, verifies only what the evidence supports, and applies explicit recovery policy.

The core rule is deliberately strict:

```text
Desired != allocated != established != observed != verified
```

No layer is allowed to infer a stronger state from a weaker one.

---

## Who it is for

Egrissa is designed for automation platforms, integration infrastructure, controlled crawlers, CI and network tests, privacy-sensitive clients, distributed agents, and geo or network behavior testing.

It is appropriate when egress must be governed as a product capability rather than configured as an untracked proxy address.

---

## What it provides

- **Policy-bound planning.** Workloads request required transport, topology, geography, identity, isolation, verification, and leakage behavior. They do not select proxy credentials or IP addresses.
- **Logical identity lifecycle.** Identities have opaque IDs, independent generations, scopes, lifetimes, and explicit continuity states: `Preserved`, `Changed`, or `Unknown`.
- **Fail-closed routing.** `ProhibitDirect` is the default. Unsupported or ineligible routes fail before establishment rather than falling back to direct connectivity.
- **Explainable route choice.** Deterministic route intelligence ranks eligible candidates from typed reliability, latency, cooldown, cost, and destination-scoped evidence, then records the decision.
- **Evidence discipline.** Provider claims are inventory input. Observations are not verification. Destination evidence cannot globally poison endpoint health.
- **Adapter boundaries.** SOCKS5, HTTP CONNECT, relay, MASQUE, and provider adapters keep addresses, protocol behavior, and credential resolution outside the domain model.
- **Concurrency and lifecycle safety.** Exactly one establishment owns a route generation. Stale feedback cannot mutate current route or identity state.

---

## System model

```text
Workload
  -> EgressRequest + EgressPolicy
  -> NetworkIdentity + RoutePlan
  -> establishment boundary
  -> immutable observations
  -> attribution and verification
  -> RouteLease or explicit recovery
```

Egrissa has three planes:

| Plane | Responsibility |
| --- | --- |
| Control | Requirements, policy, identity lifecycle, route planning, recovery |
| Data | Transport, relay, and provider adapter boundaries |
| Evidence | Observations, attribution, health projection, verification |

The control plane does not hold its mutation lock while it awaits transport or verifier work.

---

## Supported integration surfaces

| Surface | Purpose |
| --- | --- |
| `egress-core` | Requirements, policy, capability, IDs, and typed errors |
| `identity-core` | Logical identity and continuity lifecycle |
| `route-core` | Eligibility planning, topology, recovery, deterministic route intelligence |
| `observation-core` | Immutable observations, attribution, and health projection |
| `verification-core` | Evidence-bound verification contracts |
| `transport-core` | Transport-neutral establishment, tunnel, datagram, and packet contracts |
| `egress-runtime` | Asynchronous orchestration and deterministic fake transport |
| `socks5-adapter` | SOCKS5 TCP CONNECT contract adapter |
| `http-connect-adapter` | HTTP CONNECT contract adapter, including TLS-to-proxy configuration |
| `relay-adapter` | CONNECT relay client/server contract adapter with explicit forwarding policy |
| `masque-adapter` | CONNECT-UDP and CONNECT-IP contract adapter with typed protocol negotiation |
| `provider-core` | Generic provider binding lifecycle and inventory publication |
| `reference-provider-adapter` | Configurable generic provider-to-inventory adapter |

---

## Security and policy guarantees

Egrissa is designed around these non-negotiable guarantees:

1. Domain objects contain credential references, never credentials.
2. Direct fallback is prohibited unless policy explicitly permits it.
3. Route and identity generations are independent and stale callbacks are rejected.
4. Identity-changing recovery is explicit.
5. Route lifetime cannot exceed any dependency lifetime.
6. Unsupported topology is rejected before establishment.
7. Application status codes do not directly change route health.
8. Provider assertions are never verification proof.
9. Planning is deterministic unless an injected chooser explicitly introduces bounded randomness.

See [INVARIANTS.md](INVARIANTS.md) and [SECURITY.md](SECURITY.md) for the complete engineering rules.

---

## Current maturity and limits

The repository has a complete semantic kernel, deterministic fake runtime, and contract-level adapter implementations for SOCKS5, HTTP CONNECT, relay, MASQUE, and providers.

It is **not yet a production-ready managed egress service**. In particular, public release claims must not state that every concrete adapter is wired into the runtime's complete lease, observation, and verification path until that integration is independently verified. The same applies to any statement that exit-IP equality proves logical identity continuity.

Egrissa does not offer destination-specific access-control circumvention, anonymity guarantees, or IP rotation as its primary abstraction.

---

## Build and verify

Requirements:

- Rust 1.85 or later
- Cargo

Run the repository quality gate from the project root:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The test strategy includes unit, property, deterministic fake-runtime integration, adapter contract, cancellation, failure, and lifecycle coverage. See [TESTING.md](TESTING.md).

---

## Documentation map

- [BRAND.md](BRAND.md) - working commercial name, usage, and trademark-clearance steps.
- [PRD.md](PRD.md) - product requirements.
- [DOMAIN-MODEL.md](DOMAIN-MODEL.md) - canonical objects and lifecycle vocabulary.
- [ARCHITECTURE.md](ARCHITECTURE.md) - system shape and planes.
- [INVARIANTS.md](INVARIANTS.md) - non-negotiable semantic constraints.
- [SECURITY.md](SECURITY.md) - trust boundaries and abuse boundary.
- [TESTING.md](TESTING.md) - verification layers and mandatory properties.
- [IMPLEMENTATION-REPORT.md](../IMPLEMENTATION-REPORT.md) - milestone-level implementation report.
