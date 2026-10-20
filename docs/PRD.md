# PRD
## Product
A policy-driven network identity and egress orchestration runtime. Workloads declare required identity/egress properties; the runtime plans an eligible route, establishes through adapters, observes execution, verifies properties, and applies explicit recovery without silently weakening policy.

## Users
Automation/integration infrastructure, controlled crawlers, CI/network tests, privacy-sensitive clients, distributed agents, geo/network behavior tests.

## Goals
Stable logical identities; provider/transport independence; deterministic planning; explicit continuity; evidence-driven recovery; fail-closed routing; operator separation; pluggable SOCKS5/CONNECT/relay/MASQUE/providers.

## Non-goals
Destination-specific access-control circumvention; anonymity promises; IP rotation as primary abstraction; hidden uncertainty; credentials in domain objects; adaptive routing before deterministic semantics.

## Success
State transitions are executable invariants; concurrent acquisition has one establishment owner; stale callbacks cannot corrupt new generations; fail-closed cannot leak direct; real transports do not rewrite identity semantics; observation/attribution/verification remain distinct.
