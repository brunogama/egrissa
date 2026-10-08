# Egress Runtime v1 --- Domain Specification

**Status:** Proposed\
**Version:** 0.1.0\
**Date:** 2026-10-07

## 1. Product Definition

Egress Runtime lets a workload declare required network identity and
egress properties. The runtime plans an eligible route, delegates
establishment to transport/provider adapters, records observations,
verifies relevant properties, and applies recovery policy without
silently weakening requirements.

It is not a proxy rotator. Rotation is one possible policy.

### Core invariant

> Desired state != allocated state != established state != observed
> state != verified state.

No layer may infer a stronger state from a weaker one.

### Non-goals

-   No SOCKS5, HTTP CONNECT, MASQUE, QUIC, TLS, DNS, or provider
    protocol implementation in domain crates.
-   No provider credentials in domain objects.
-   No assumption that HTTP status codes prove endpoint or IP health.
-   No silent direct fallback.
-   No promise of location, rotation, anonymity, or continuity unless
    verified.
-   No destination-specific restriction-circumvention policy.

## 2. Architecture

``` text
Workload -> EgressRequest
                |
                v
+---------- CONTROL PLANE ----------+
| Identity Manager                  |
| Policy Engine                     |
| Capability Matcher                |
| Route Planner                     |
| Allocation / Lease Manager        |
+----------------+------------------+
                 | RoutePlan
                 v
+----------- DATA PLANE ------------+
| Transport + Provider Adapters     |
| SOCKS5 | CONNECT | MASQUE | Relay |
+----------------+------------------+
                 |
                 v
              Internet
                 |
                 v
+-------- OBSERVATION PLANE --------+
| Observation -> Attribution        |
|             -> Verification       |
+-----------------------------------+
```

## 3. Canonical Domain Objects

The seven canonical objects are:

1.  `EgressRequest`
2.  `NetworkIdentity`
3.  `EgressPolicy`
4.  `RoutePlan`
5.  `Observation`
6.  `FailureAttribution`
7.  `VerifiedEgress`

Everything else supports these concepts.

## 4. Typed IDs

``` rust
macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub u128);
    };
}

id_type!(WorkloadId);
id_type!(RequestId);
id_type!(NetworkIdentityId);
id_type!(RouteId);
id_type!(EndpointId);
id_type!(OperatorId);
id_type!(ProviderId);
id_type!(CredentialRef);
id_type!(UpstreamBindingId);
id_type!(ObservationId);
id_type!(VerificationId);
```

IDs are correlation keys, never credentials or authorization tokens.

## 5. EgressRequest

Applications declare requirements, not proxies.

``` rust
pub struct EgressRequest {
    pub id: RequestId,
    pub workload: WorkloadId,
    pub destination: Destination,
    pub identity: IdentityRequirement,
    pub transport: TransportRequirement,
    pub geography: GeographyRequirement,
    pub isolation: IsolationRequirement,
    pub privacy: PrivacyRequirement,
}
```

``` rust
pub enum IdentityRequirement {
    Ephemeral,
    Sticky { minimum_lifetime: Duration },
    Existing(NetworkIdentityId),
    Isolated,
}

pub enum TransportRequirement {
    Tcp,
    Udp,
    Ip,
    AnyOf(TransportSet),
}

pub enum IsolationRequirement {
    Shared,
    PerSession,
    PerWorkload,
    Dedicated,
}

pub enum GeographyRequirement {
    Any,
    Country(CountryCode),
    Region(RegionCode),
}

pub enum PrivacyRequirement {
    Standard,
    SeparateClientAndDestinationKnowledge,
    DistinctOperators,
}
```

`Sticky` requests continuity. It does not inherently mean a fixed IP.

## 6. NetworkIdentity

A network identity is a logical continuity domain. It is not an IP
address.

``` rust
pub struct NetworkIdentity {
    pub id: NetworkIdentityId,
    pub generation: u64,
    pub workload: WorkloadId,
    pub scope: IdentityScope,
    pub created_at: Instant,
    pub valid_until: Option<Instant>,
    pub state: IdentityState,
}

pub enum IdentityState {
    Allocating,
    Active,
    Degraded,
    Invalidated,
    Expired,
}

pub enum IdentityContinuity {
    Preserved,
    Changed,
    Unknown,
}
```

Uncertainty MUST remain `Unknown`.

Identity generation changes only when the runtime accepts a
continuity-breaking replacement. Route generation is separate.

## 7. EgressPolicy

``` rust
pub struct EgressPolicy {
    pub identity: IdentityPolicy,
    pub routing: RoutingPolicy,
    pub isolation: IsolationPolicy,
    pub recovery: RecoveryPolicy,
    pub verification: VerificationPolicy,
    pub leakage: LeakagePolicy,
}

pub enum RecoveryPolicy {
    FailClosed,
    RetrySameRoute { attempts: u8 },
    ReplanPreservingIdentity,
    ReplanAllowingIdentityChange,
}

pub enum LeakagePolicy {
    ProhibitDirect,
    AllowDirect,
}
```

Default leakage policy is `ProhibitDirect`.

There is deliberately no `rotate_on_403`. Application-protocol
observations do not directly control topology.

## 8. Capability Model

Adapters advertise implementation capabilities.

``` rust
pub struct EgressCapabilities {
    pub transport: TransportCapabilities,
    pub identity: IdentityCapabilities,
    pub topology: TopologyCapabilities,
    pub geography: GeographyCapabilities,
    pub verification: VerificationCapabilities,
}

pub struct TransportCapabilities {
    pub tcp: bool,
    pub udp: bool,
    pub ip: bool,
    pub http_connect: bool,
    pub connect_udp: bool,
    pub connect_ip: bool,
    pub http2: bool,
    pub http3: bool,
}

pub struct IdentityCapabilities {
    pub sticky: bool,
    pub maximum_sticky_ttl: Option<Duration>,
    pub continuity_observable: bool,
}
```

Capabilities are planning facts, not establishment evidence.

## 9. RoutePlan

A route plan is immutable desired state.

``` rust
pub struct RoutePlan {
    pub id: RouteId,
    pub request: RequestId,
    pub identity: NetworkIdentityId,
    pub identity_generation: u64,
    pub route_generation: u64,
    pub topology: RouteTopology,
    pub created_at: Instant,
    pub expires_at: Instant,
}
```

v1 uses explicit supported topology shapes rather than arbitrary hop
arrays.

``` rust
pub enum RouteTopology {
    Socks5 {
        gateway: EndpointId,
        upstream: UpstreamBindingId,
    },
    HttpConnect {
        proxy: EndpointId,
    },
    Relay {
        relay: EndpointId,
    },
    RelayThenSocks5 {
        relay: EndpointId,
        gateway: EndpointId,
        upstream: UpstreamBindingId,
    },
    TwoRelaysThenSocks5 {
        ingress: EndpointId,
        egress: EndpointId,
        gateway: EndpointId,
        upstream: UpstreamBindingId,
    },
}
```

Route expiry is bounded by every dependency's validity.

## 10. Planning

``` rust
pub trait RoutePlanner {
    fn plan(
        &self,
        request: &EgressRequest,
        policy: &EgressPolicy,
        inventory: &InventorySnapshot,
        observations: &ObservationSnapshot,
        chooser: &mut impl CandidateChooser,
    ) -> Result<RoutePlan, PlanningError>;
}
```

Candidate filtering order:

1.  destination eligibility;
2.  transport capabilities;
3.  identity compatibility;
4.  topology support;
5.  endpoint enabled state;
6.  upstream availability;
7.  health/cooldown eligibility;
8.  isolation;
9.  operator separation;
10. geography;
11. lifetime feasibility;
12. deterministic ranking;
13. optional stochastic selection among equivalent candidates.

No eligible route produces a typed failure, not direct fallback.

## 11. Observation

Observation is immutable evidence, not interpretation.

``` rust
pub struct Observation {
    pub id: ObservationId,
    pub route: RouteId,
    pub route_generation: u64,
    pub identity: NetworkIdentityId,
    pub identity_generation: u64,
    pub observed_at: Instant,
    pub scope: ObservationScope,
    pub outcome: Outcome,
}

pub enum ObservationScope {
    Transport,
    Endpoint(EndpointId),
    Operator(OperatorId),
    Provider(ProviderId),
    Upstream(UpstreamBindingId),
    Destination(DestinationKey),
    Route(RouteId),
    Identity(NetworkIdentityId),
}

pub enum Outcome {
    EstablishmentStarted,
    Established,
    ConnectTimeout,
    ConnectionReset,
    AuthenticationRejected,
    HttpStatus(u16),
    EgressObserved(EgressObservation),
    VerificationPassed(VerificationId),
    VerificationFailed(VerificationId),
}
```

An HTTP 403 does not prove endpoint, relay, provider, or identity
failure.

## 12. FailureAttribution

``` rust
pub struct FailureAttribution {
    pub category: FailureCategory,
    pub confidence: Confidence,
    pub evidence: Vec<ObservationId>,
    pub affected_scope: FailureScope,
}

pub enum FailureCategory {
    TransportFailure,
    EndpointUnavailable,
    ProviderUnavailable,
    UpstreamAuthentication,
    DestinationRejected,
    RateLimitSuspected,
    IdentityReputationSuspected,
    VerificationFailure,
    Unknown,
}

pub enum Confidence {
    Confirmed,
    Strong,
    Weak,
    Unknown,
}
```

Attribution may guide recovery but cannot manufacture evidence.

## 13. VerifiedEgress

Verification records properties actually established by evidence.

``` rust
pub struct VerifiedEgress {
    pub id: VerificationId,
    pub route: RouteId,
    pub route_generation: u64,
    pub identity: NetworkIdentityId,
    pub identity_generation: u64,
    pub verified_at: Instant,
    pub properties: VerifiedProperties,
}

pub struct VerifiedProperties {
    pub exit_address: Option<IpAddr>,
    pub asn: Option<Asn>,
    pub country: Option<CountryCode>,
    pub transport: Option<ObservedTransport>,
    pub identity_continuity: IdentityContinuity,
}
```

`None` means not verified, not false. Provider declarations never become
verification automatically.

## 14. Lifecycles

``` text
Route:
Unallocated -> Planned -> Establishing -> Ready
                              |
                              +-> Failed
Ready -> Expired

Identity:
Allocating -> Active
               |
               +-> Degraded -> Active
               |       |
               |       +-> Invalidated
               |
               +-> Expired
```

Replacement is explicit:

``` rust
pub enum ReplacementDecision {
    PreserveIdentity(RoutePlan),
    ReplaceIdentity {
        old: NetworkIdentityId,
        new: NetworkIdentity,
        route: RoutePlan,
    },
    Reject(RecoveryError),
}
```

## 15. Acquire Contract

``` rust
pub enum AcquireDecision {
    Establish(RoutePlan),
    Reuse(RouteLease),
    Pending {
        route: RouteId,
        route_generation: u64,
    },
    Reject(AcquireError),
}
```

The manager stays synchronous. Network I/O happens outside its mutation
lock.

Every asynchronous callback carries: - route ID; - route generation; -
identity ID; - identity generation.

Stale feedback may remain historical evidence but cannot mutate current
state.

## 16. Security and Correctness Invariants

1.  Intent is never evidence.
2.  Allocation is never establishment.
3.  Establishment is never verification.
4.  Unknown continuity never becomes preserved without evidence.
5.  Route and identity generations are independent.
6.  Stale feedback cannot mutate current state.
7.  Domain objects contain credential references only.
8.  No silent direct fallback under `ProhibitDirect`.
9.  Destination failures do not automatically poison global endpoint
    health.
10. Provider claims are not verification.
11. Unsupported topology is rejected before network establishment.
12. Operator separation is validated before allocation.
13. Identity-changing recovery is explicit.
14. Route expiry cannot exceed dependency validity.
15. Exactly one establishment exists per route generation.
16. Concurrent acquire during establishment returns `Pending`.
17. No manager lock is held across network await.
18. Logs/events use typed or redacted destination and credential
    representations.
19. Recovery is driven by policy plus evidence, not raw status-code
    heuristics.
20. Failure to observe is not proof of absence.

## 17. Workspace

``` text
crates/
├── egress-core/
├── identity-core/
├── route-core/
├── observation-core/
├── verification-core/
├── transport-core/
├── transport-socks5/
├── transport-http-connect/
├── transport-masque/
├── provider-core/
├── relay-client/
├── relay-server/
├── egress-runtime/
├── local-proxy/
└── egress-cli/
```

Recommended dependency direction:

``` text
             egress-core
          /      |       \
 identity-core route-core observation-core
          \      |       /
           \     |      /
             egress-runtime
                  |
       transport/provider adapters
```

Domain crates MUST NOT depend on concrete transports.

## 18. Crate Responsibilities

### egress-core

Requests, policies, capabilities, decisions, common typed errors.

### identity-core

Identity lifecycle, continuity semantics, affinity, generations.

### route-core

Topology, route plans, planning, allocation, session pinning.

### observation-core

Immutable observations, health projections, failure attribution.

### verification-core

Verification contracts and verified egress records.

### transport-core

Traits and transport-neutral establishment/result contracts.

### provider-core

Opaque provider binding lifecycle and capability exposure.

### egress-runtime

Orchestration across domain kernels and adapters.

### local-proxy

Local SOCKS/HTTP entry point. No policy ownership.

## 19. Acceptance Tests

  -----------------------------------------------------------------------
  Test                                Required behavior
  ----------------------------------- -----------------------------------
  repeated acquire                    compatible request reuses eligible
                                      identity/route

  concurrent acquire                  exactly one establishment; others
                                      pending

  route expiration                    `now == expires_at` rejects new use

  no candidate                        typed rejection; no implicit direct
                                      path

  stale route feedback                old route generation cannot alter
                                      current route

  stale identity feedback             old identity generation cannot
                                      alter current identity

  route replacement                   may preserve identity without
                                      changing identity generation

  identity replacement                increments identity generation
                                      explicitly

  operator separation                 invalid combinations never become
                                      candidates

  unsupported transport               rejected before establishment

  lifetime bound                      route validity cannot outlive
                                      dependencies

  HTTP 403                            does not globally mark relay
                                      unhealthy

  provider claim                      does not create `VerifiedEgress`

  verification failure                represented independently of
                                      transport establishment

  unknown continuity                  remains unknown without evidence

  fail closed                         transport failure cannot produce
                                      direct connection

  secrets                             plans/events contain opaque
                                      credential references only

  deterministic planner               same snapshots + inputs produce
                                      same ranking

  random policy                       randomness enters only through
                                      injected chooser

  lock discipline                     network work never occurs while
                                      manager mutation lock is held
  -----------------------------------------------------------------------

## 20. Property-Based Invariants

Use `proptest` for: - no valid transition bypasses lifecycle rules; -
stale generations never mutate current state; - route expiry is always
\<= all supplied validity bounds; - distinct-operator policies never
emit equal operators; - planner never returns a candidate missing
required capabilities; - fail-closed policy never emits a direct
route; - replacement that changes identity cannot report `Preserved`; -
arbitrary observations cannot become verification without verifier
output.

## 21. v1 Build Order

### Phase A --- Semantic kernel

1.  IDs and value objects
2.  `EgressRequest`
3.  `NetworkIdentity`
4.  `EgressPolicy`
5.  `RoutePlan`
6.  lifecycle transitions
7.  property-based invariant suite

### Phase B --- Planning

8.  capability inventory
9.  candidate generation
10. deterministic ranking
11. injected chooser
12. route/identity leases

### Phase C --- Evidence

13. observations
14. attribution
15. verification
16. health projections

### Phase D --- Runtime

17. synchronous manager
18. async orchestration shell
19. transport traits
20. one reference adapter

### Phase E --- Integration

21. SOCKS5 adapter
22. HTTP CONNECT adapter
23. MASQUE/relay adapter
24. provider adapters
25. CLI/local proxy

Do not begin with MASQUE. Prove identity, planning, generations,
evidence, and fail-closed behavior using a fake transport first.

## 22. v1 Definition of Done

v1 is complete when:

-   a caller can submit an `EgressRequest`;
-   policy and capabilities produce a deterministic eligible
    `RoutePlan`;
-   one logical `NetworkIdentity` can survive a route replacement when
    continuity is verified;
-   continuity loss is explicit;
-   establishment, observations, attribution, and verification remain
    separate facts;
-   concurrent acquisition cannot duplicate establishment;
-   stale callbacks cannot corrupt current state;
-   direct fallback cannot occur accidentally;
-   a fake transport proves the complete state machine;
-   at least one real transport adapter can establish traffic without
    adding transport semantics to domain crates.

## 23. Key Architectural Decision

The fundamental aggregate is not:

``` text
Session -> Proxy
```

It is:

``` text
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

This preserves the central property of the system: it can distinguish
what was requested, what it attempted, what happened, what it knows, and
what it is allowed to do next.
