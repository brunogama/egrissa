# Milestone Proof Matrix

This is the executable proof plan for the M0–M7 roadmap. It is intentionally
conservative: a source file or a passing compile is not proof of a milestone.
A milestone is **demonstrated** only when its named commands pass against the
working tree being evaluated.

## Scope and evidence rules

- Requirements come from `milestones/M0-SEMANTIC-KERNEL.md` through
  `milestones/M7-ROUTE-INTELLIGENCE.md`; the invariant baseline is
  `docs/INVARIANTS.md`.
- The common gate establishes formatting, lint cleanliness, and whole-workspace
  regression coverage. Each milestone also has a focused behavioral command.
- A passing adapter contract proves only its local-fixture behavior. It does
  not prove production deployment, external interoperability, or anonymity.
- Source paths below are review anchors. Test names and commands are the
  executable evidence.
- Run all commands from the repository root. Do not reuse a historical result
  after changing source, dependencies, a toolchain, or test configuration.

## Common gate

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The final command is the broad regression proof. Run the focused commands
below as well: they make the milestone behavior being claimed observable.

## Current checked snapshot

On `2026-10-20T11:33:05Z`, the common gate and every focused command in this
document completed successfully on branch `ci/fix-toolchain-install` at
`b339a666a1079fe335a6b215cab69ce3f180f90a`. The worktree intentionally
contained pre-existing uncommitted documentation changes, so this is evidence
for that exact workspace snapshot, not a clean-commit attestation.

## M0 — semantic kernel

**Claim.** Requests, policies, capabilities, network identity, explicit
non-direct topologies, immutable routes, deterministic planning, injected
selection, independent generations, and fail-closed planning exist without
real network I/O.

| Evidence | Location |
| --- | --- |
| Request, policy, capability, ID, and typed-error model | `crates/egress-core/src/` |
| Identity, continuity, lifecycle, and generation rules | `crates/identity-core/src/` |
| Topology, route plan, planner, and lifecycle rules | `crates/route-core/src/` |
| Cross-cutting M0/property scenarios | `tests/m0.rs`, `tests/planning.rs`, `tests/acquisition.rs`, `tests/recovery.rs`, `tests/properties.rs` |
| Topology behavior | `crates/route-core/tests/topologies.rs` |

```sh
cargo test -p route-core --test m0 --test planning --test acquisition --test recovery --test properties --test topologies
cargo test -p identity-core
cargo test -p egress-core
```

**Observed behavior to inspect.** No candidate or unsupported topology is
rejected before establishment; `ProhibitDirect` cannot emit a direct route;
expired or stale route state cannot be accepted; equivalent planner inputs keep
the same order unless the explicitly injected chooser decides otherwise.

**Boundary.** The source/manifests provide a reviewable no-I/O boundary, but
there is not yet a dedicated automated dependency/no-network-I/O policy check.

## M1 — deterministic fake runtime

**Claim.** The fake runtime coordinates asynchronous establishment, shared
pending acquisition, leases, observations, verification, attribution, recovery,
expiry, cancellation, shutdown, and generation-safe callbacks without holding
the manager mutation lock across an await.

| Evidence | Location |
| --- | --- |
| Scripted deterministic transport | `crates/egress-runtime/src/fake.rs` |
| Runtime coordination and retained-tunnel lifecycle | `crates/egress-runtime/src/lib.rs` |
| Evidence, attribution, and health projection | `crates/observation-core/src/` |
| Evidence-bound verification | `crates/verification-core/src/lib.rs` |
| Runtime scenarios, including 100 shared acquires | `crates/egress-runtime/tests/m1_runtime.rs` |
| Verification/reuse and retained-tunnel regressions | `crates/egress-runtime/tests/verification_safety.rs`, `crates/egress-runtime/tests/transport_wrappers.rs` |

```sh
cargo test -p egress-runtime --test m1_runtime --test verification_safety --test transport_wrappers
cargo test -p observation-core
cargo test -p verification-core --test evidence_verifier
```

**Observed behavior to inspect.**
`m1_05_one_hundred_acquires_share_one_establishment` is the direct proof that
100 compatible acquires share exactly one establishment. The neighboring
named runtime scenarios exercise waiter wakeup, cancellation, replacement,
expiry, stale success/failure, and shutdown. Verification must remain distinct
from establishment and cannot be manufactured by observations.

**Boundary.** The fake transport proves orchestration semantics, not a real
network path. M1 has scenario coverage; it does not have a separately named
runtime `proptest` suite.

## M2 — SOCKS5 TCP CONNECT

**Claim.** The SOCKS5 adapter resolves credentials outside the domain, performs
TCP CONNECT negotiation, returns a usable tunnel, and fails closed before a
connection when credentials are unavailable.

| Evidence | Location |
| --- | --- |
| SOCKS5 framing, authentication, resolver boundary, and tunnel | `crates/socks5-adapter/src/lib.rs` |
| Local SOCKS5 fixture contract | `crates/socks5-adapter/tests/contract.rs` |
| Runtime wrapper lifecycle | `crates/egress-runtime/tests/transport_wrappers.rs` |
| Design decision preserving transport semantics | `docs/adr/0021.md` |

```sh
cargo test -p socks5-adapter --test contract
cargo test -p egress-runtime --test transport_wrappers socks5_adapter_wrapper_establishes_verifies_and_closes_the_retained_tunnel
```

**Observed behavior to inspect.**
`connect_with_username_password_yields_a_usable_tunnel` checks SOCKS greeting,
authentication, CONNECT framing, and bidirectional `ping`/`pong` tunnel I/O.
`missing_credential_reference_is_rejected_before_connection` demonstrates
fail-closed credential resolution.

**Boundary.** This local fixture does not yet exercise malformed replies,
unsupported authentication, server-side authentication rejection, or all
CONNECT-refusal paths.

## M3 — HTTP CONNECT

**Claim.** The HTTP CONNECT adapter supports adapter-local credentials, typed
failure mapping, optional TLS-to-proxy configuration, configurable timeouts,
and a usable authenticated local tunnel without exposing raw proxy details to
the domain.

| Evidence | Location |
| --- | --- |
| HTTP CONNECT, TLS configuration, timeout, and sanitised failure surface | `crates/http-connect-adapter/src/lib.rs` |
| Local authenticated CONNECT fixture | `crates/http-connect-adapter/tests/contract.rs` |
| Adapter security/architecture boundary | `docs/adr/0022.md` |

```sh
cargo test -p http-connect-adapter --test contract
cargo test -p http-connect-adapter --test contract local_http_connect_proxy_yields_a_usable_authenticated_tunnel
cargo test -p http-connect-adapter --test contract missing_credential_fails_closed_before_proxy_connection
```

**Observed behavior to inspect.** The successful scenario observes a CONNECT
request with Basic authentication, a 2xx response, and bidirectional tunnel
I/O; the failure scenario observes a missing credential rejected before proxy
connection.

**Unproven required edges.** The implementation has TLS and timeout controls,
but the local contract suite does not currently exercise TLS roots/handshake
failure, proxy `407` or other non-2xx responses, malformed responses, a
stalled timeout, cancellation, or an explicit secret-redaction assertion.
Treat those as proof gaps rather than as established behavior.

## M4 — relay

**Claim.** Relay client/server support one- and two-relay CONNECT paths,
allowlist-based forwarding restrictions, fail-closed credentials, planner
operator separation, and safe rejection of UDP relay routes.

| Evidence | Location |
| --- | --- |
| Relay client, server, CONNECT parser, and forwarding policy | `crates/relay-adapter/src/lib.rs` |
| Local relay fixtures and one/two-hop contracts | `crates/relay-adapter/tests/contract.rs` |
| Relay topology and planner constraints | `crates/route-core/src/topology.rs`, `crates/route-core/src/planner.rs` |
| UDP/operator-separation checks | `crates/route-core/tests/topologies.rs` |

```sh
cargo test -p relay-adapter --test contract
cargo test -p route-core --test topologies relay_topologies_reject_udp_before_establishment
cargo test -p route-core --test topologies relay_planning_rejects_same_operator_hops_when_separation_is_required
```

**Observed behavior to inspect.** The contract suite names local tests for
allowlisted targets, rejected non-CONNECT requests, one/two relay hops, and
credential failure before connecting. The route-core tests prove that an
unsupported UDP relay route and prohibited shared-operator route do not reach
establishment.

**Boundary.** Current tests do not demonstrate server-side relay credential
authorization, distinct per-hop credentials, or successful capability-enabled
UDP relay forwarding. Rejection of unsupported UDP is fail-closed evidence,
not proof of UDP forwarding.

## M5 — MASQUE

**Claim.** The adapter maps canonical UDP/IP plans to CONNECT-UDP or CONNECT-IP,
uses HTTP/3 first, permits HTTP/2 only through explicit advertised fallback,
and fails closed after a request has been rejected or credentials are missing.

| Evidence | Location |
| --- | --- |
| MASQUE advertisement, fallback, request, and connector contract | `crates/masque-adapter/src/lib.rs` |
| CONNECT-UDP/IP, downgrade, and fail-closed contracts | `crates/masque-adapter/tests/contract.rs` |
| Generic transport capability/topology constraints | `crates/egress-core/src/request.rs`, `crates/route-core/src/topology.rs`, `crates/route-core/tests/topologies.rs` |

```sh
cargo test -p masque-adapter --test contract
cargo test -p route-core --test topologies http_connect_udp_requires_connect_udp_and_an_explicit_http_version
cargo test -p route-core --test topologies http_connect_ip_requires_connect_ip_and_an_explicit_http_version
```

**Observed behavior to inspect.** The named contract tests prove in-memory
CONNECT-UDP datagrams, CONNECT-IP packets, allowed pre-request HTTP/2 fallback,
and no fallback after rejected HTTP/3 requests or missing credentials.

**Boundary.** `MasqueConnector` is injected; these are not wire-level HTTP/3,
QUIC, capsule, or external-peer interoperability proofs. The planner's
HTTP/2-only eligibility and the adapter's HTTP/3-required advertisement should
be reconciled before claiming a complete capability model.

## M6 — providers

**Claim.** Generic provider bindings carry capability advertisements, sticky
validity, credential references, and deterministic availability/cooldown
projection; the configurable reference adapter publishes a generic inventory.

| Evidence | Location |
| --- | --- |
| Generic binding lifecycle and availability projection | `crates/provider-core/src/lib.rs` |
| Provider lifecycle/property tests | `crates/provider-core/tests/binding.rs` |
| Configurable reference inventory projection | `crates/reference-provider-adapter/src/lib.rs` |
| Reference-provider contract | `crates/reference-provider-adapter/tests/contract.rs` |

```sh
cargo test -p provider-core --test binding
cargo test -p reference-provider-adapter --test contract
```

**Observed behavior to inspect.** The property test
`observation_projection_is_input_order_independent` proves deterministic
availability projection. The reference contract checks sticky capabilities,
validity bounded by a credential lease, opaque credential-reference propagation,
and planning rejection after an unavailable observation.

**Boundary.** “Configurable” currently means an in-memory configuration value;
there is no file/environment/control-plane configuration path or end-to-end
runtime provider lifecycle proof. The architecture boundary is structural,
not currently enforced by a dedicated dependency test.

## M7 — route intelligence

**Claim.** The planner can rank hard-eligible routes deterministically from
scoped reliability, latency, cooldown, cost, and destination evidence, return
an explainable decision record, and use adaptive selection only behind explicit
policy with deterministic fallback.

| Evidence | Location |
| --- | --- |
| Scoring, exclusion, decision-record, and selection-policy model | `crates/route-core/src/intelligence.rs` |
| Planner integration after hard eligibility | `crates/route-core/src/planner.rs` |
| Ranking/property/integration scenarios | `tests/route_intelligence.rs` |
| Existing focused implementation matrix | `IMPLEMENTATION-REPORT.md` |

```sh
cargo test -p route-core --test route_intelligence
```

**Observed behavior to inspect.** The suite directly covers reliability versus
static rank, latency/cost scoring, destination-only reordering, evidence-bound
cooldown exclusion, invalid adaptive-choice fallback, planning after hard
eligibility, and input-permutation deterministic ranking.

**Boundary.** Evidence IDs are retained in score inputs, but no repository path
currently proves ingestion from `observation-core` into route intelligence.
Cost is a caller input rather than a pluggable provider interface. Decision
records are returned to callers but not persisted or emitted as telemetry.

## Completion checklist

Before reporting any milestone as proved for a revision:

1. Start from a clean or intentionally recorded working tree (`git status --short`).
2. Run the common gate and every focused command above.
3. Confirm every named command exits zero; preserve the terminal output in the
   review or CI record for that revision.
4. Read the relevant **Boundary** section. Do not promote a local contract to a
   stronger operational claim.
5. When a new scenario closes a stated gap, add both the test and its exact
   proof command here.
