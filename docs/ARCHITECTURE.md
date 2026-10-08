# Architecture

## Product

Egress Runtime is a policy-driven network identity and egress orchestration runtime.

Applications declare required properties. They do not select proxies directly.

```text
Workload
  -> EgressRequest
  -> NetworkIdentity
  -> RoutePlan
  -> Establishment
  -> Observation
  -> Attribution
  -> Verification
  -> Recovery
```

The central rule is:

```text
desired != allocated != established != observed != verified
```

## Planes

### Control plane
Identity management, policy, capabilities, planning, leases.

### Data plane
Future transport/provider adapters such as SOCKS5, HTTP CONNECT, MASQUE and relays.

### Observation plane
Immutable observations, failure attribution and verified egress.

## Key abstractions

`NetworkIdentity` is logical continuity, not an IP address.

`EgressRequest` describes requirements, not implementation.

`EgressPolicy` controls identity, routing, isolation, recovery, verification and leakage.

`RoutePlan` is immutable desired routing state.

`Observation` records evidence.

`FailureAttribution` interprets evidence while preserving uncertainty.

`VerifiedEgress` records properties actually proven.

## Fail closed

Direct network access must never appear merely because an eligible route failed.

Default leakage policy: `ProhibitDirect`.

## Evidence

A provider claim is inventory/planning input.

It is not verification.

An HTTP 403 is an observation.

It is not proof that a proxy, relay, endpoint or identity is globally unhealthy.
