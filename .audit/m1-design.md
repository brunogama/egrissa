# M1 Design Sketch - Fake End-to-End Runtime

Working artifact. Grounded in `docs/egress-runtime-v1-domain-spec.md`, `CRAFT-M1.md`, and the M0 tree.

## 1. Crate graph after M1

```text
egress-core          (thiserror)                - unchanged deps
identity-core        -> egress-core             - VerifiedEgress moves OUT
observation-core     -> egress-core             - NEW, evidence rehomed here
route-core           -> egress-core, identity-core, observation-core
verification-core    -> egress-core, identity-core, route-core
transport-core       -> egress-core, route-core
egress-runtime       -> all above + tokio(rt,sync,macros)   - only crate with tokio
```

Domain crates get NO tokio, NO serde, NO concrete RNG. New cross-domain edges
(route->observation, verification->identity+route, transport->route) follow the
M0 precedent (route-core already depends on identity-core) and stay within the
domain layer. The spec's dependency tree is "recommended"; the hard rules are
domain purity and adapter direction.

## 2. Type migrations (one wave, no re-exports)

- `egress_core::evidence::*` -> `observation_core`: ObservationScope,
  EgressObservation, Outcome, Observation. egress-core::evidence deleted.
- `route_core::planner::ObservationSnapshot` -> `observation_core` (it is
  attributed health input; observation-core owns health projections per spec 18).
- `identity_core::continuity::{VerifiedEgress, VerifiedProperties}` ->
  `verification_core`. IdentityContinuity stays in identity-core.
- M0's compile-fail doc tests (no conversion from capabilities/observations to
  VerifiedEgress) relocate to verification-core.

## 3. observation-core additions

```rust
pub enum FailureScope { Transport, Endpoint(EndpointId), Operator(OperatorId),
    Provider(ProviderId), Upstream(UpstreamBindingId), Destination(DestinationKey),
    Route(RouteId), Identity(NetworkIdentityId) }
// FailureAttribution.affected_scope becomes FailureScope (spec 12)
```

Attribution engine, table-driven and deterministic:

```rust
pub fn attribute(observations: &[Observation]) -> Option<FailureAttribution>
```

Rules (scope comes from the recorded observation; the runtime chooses scope
from plan topology when recording):
- (Upstream, AuthenticationRejected) -> UpstreamAuthentication / Confirmed
- (Endpoint, AuthenticationRejected) -> EndpointUnavailable / Strong
- (any, ConnectTimeout | ConnectionReset) -> TransportFailure / Weak
- (Destination, HttpStatus 403/4xx) -> DestinationRejected / Weak
- (Destination, HttpStatus 429) -> RateLimitSuspected / Weak
- (Route, VerificationFailed) -> VerificationFailure / Confirmed
- two or more corroborating observations -> confidence bumped Weak->Strong
- no failure evidence -> None (unknown stays unknown)

Health projection (separate from raw evidence):

```rust
pub struct HealthPolicy { pub rate_limit_cooldown: Duration }
pub struct HealthProjection { unavailable_endpoints: BTreeSet<EndpointId>,
    unavailable_upstreams: BTreeSet<UpstreamBindingId>,
    endpoint_cooldowns: BTreeMap<EndpointId, Instant> }
impl HealthProjection {
    pub fn project(attributions: &[FailureAttribution], now, policy) -> Self
    pub fn into_snapshot(self) -> ObservationSnapshot
}
```

Rules: EndpointUnavailable at Strong/Confirmed marks the endpoint unavailable.
UpstreamAuthentication at Confirmed marks the upstream unavailable.
RateLimitSuspected sets a bounded cooldown. DestinationRejected, HTTP statuses,
and TransportFailure NEVER mark endpoint/provider/upstream globally unhealthy
(invariant 9). Unknown confidence projects nothing.

## 4. verification-core

```rust
// moved shapes (key-based)
pub struct VerifiedEgress { id, key: FeedbackKey, verified_at, properties, evidence: Vec<ObservationId> }
pub struct VerifiedProperties { exit_address, asn, country, transport, identity_continuity }

pub struct VerificationSubject { request: EgressRequest, plan: RoutePlan,
    identity: NetworkIdentity, observations: Vec<Observation>, now: Instant }
pub enum VerificationOutcome { Verified(VerifiedEgress), Failed(Contradicted),
    InsufficientEvidence }   // missing evidence = unverified, not false
pub trait EgressVerifier: Send + Sync {
    fn verify(&self, subject) -> boxed future (std-only, mirrors transport-core)
}

pub enum ContinuityAssessment { Preserved { evidence: Vec<ObservationId> }, Unknown }
pub fn assess_continuity(v: &VerifiedEgress) -> ContinuityAssessment
```

`EvidenceVerifier` (the M1 reference verifier, evidence-driven, no network):
- exit_address verified iff an EgressObserved{exit_address: Some} observation
  exists for the current key; country/asn/transport likewise; each cites its
  observation IDs as evidence.
- continuity Preserved iff two EgressObserved observations at distinct times
  agree on exit_address AND the identity generation is unchanged between them.
  Otherwise Unknown. No promotion from provider claims or intent.
- properties without evidence stay None (not false).

## 5. transport-core

```rust
pub struct EstablishmentRequest { plan: RoutePlan, destination: Destination, transport: Transport }
pub enum TransportFailure { ConnectTimeout, ConnectionReset, AuthenticationRejected, UnsupportedTopology }
pub enum TransportOutcome { Established, Failed(TransportFailure) }
pub trait Transport: Send + Sync + 'static {
    fn establish(&self, EstablishmentRequest) -> Pin<Box<dyn Future<Output=TransportOutcome> + Send>>;
    fn close(&self, FeedbackKey) -> Pin<Box<dyn Future<Output=()> + Send>>;
}
```

Std-only boxed futures keep domain purity and dyn compatibility. Credential
refs only (RoutePlan already holds CredentialRef). Concrete transport errors
are mapped to typed observations at the runtime boundary.

## 6. egress-runtime

Control plane behind ONE std::sync::Mutex (never held across .await; futures
must be Send so the compiler enforces it structurally):

```rust
struct ControlPlane { manager: RouteManager, identities: BTreeMap<NetworkIdentityId, NetworkIdentity>,
    next_ids: Counters, inventory: InventorySnapshot, health: ObservationSnapshot,
    verifications: Vec<VerifiedEgress>, establishments: HashMap<FeedbackKey, EstablishmentShared> }
```

Identity policy on acquire:
- Existing(id): must exist and be usable.
- Ephemeral/Sticky/Isolated: reuse a compatible usable identity, else allocate
  and transition to Active before planning. Identity allocation/activation is
  distinct from route establishment; a failed route does not imply an established
  route.

Acquire pipeline (steps from CRAFT-M1):
1. lock CP: validate, allocate identity, plan context, manager.acquire ->
   Establish(plan) | Reuse(lease) | Pending | Reject. Emit events. Drop lock.
2. Reuse: optional verification already recorded; return lease.
3. While holding the CP lock, create/register EstablishmentShared keyed by
   FeedbackKey and attach the owner's oneshot before releasing the lock. For
   Pending, attach the waiter to that exact FeedbackKey under the same lock.
   Spawn the owner pipeline only after releasing the lock.
4. Pipeline task (no CP lock across awaits): emit EstablishmentStarted; record
   EstablishmentStarted observation (typed); await transport.establish.
5. Lock CP: map TransportOutcome -> typed Observation (scope from plan
   topology: upstream binding if present, else endpoint), record via manager;
   manager.complete(key, result, now).
6. Success: clone an owned VerificationSubject under the lock when configured,
   await verification without the lock, then re-lock and reject stale output
   before recording it. RequireBeforeUse with no verifier is rejected before
   allocation; with insufficient/failed evidence it returns an error without
   changing establishment history or route state. Emit LeaseIssued only when
   policy permits.
7. Failure: attribute the generation's observations, project health and update
   ControlPlane.health before replanning. FailClosed invalidates the identity,
   removes the terminal failed allocation after retaining historical evidence,
   and settles error. RetrySameRoute / Replan* call manager.recover with
   evidence-backed continuity, then loop to step 4 (attempt-limited retry,
   one replan). A completion arriving at/after expiry settles Expired, does not
   recover, and issues no lease.
8. Settle: store chain outcome in EstablishmentShared, wake all waiters
   (oneshot), remove map entry. Idempotent settle; late waiters re-acquire and
   hit Reuse.

Continuity supplied to recover():
- Preserved is supplied only by `assess_continuity` from a current-generation
  VerifiedEgress carrying continuity evidence. Matching runtime IDs alone are
  insufficient.
- Without verified continuity, ReplanPreservingIdentity is rejected as
  ContinuityUnproven. Identity-changing replacement (only under
  ReplanAllowingIdentityChange via NetworkIdentity::replace) uses Unknown; it
  never auto-promotes to Preserved.

EstablishmentShared + waiters:
struct EstablishmentShared { result: Option<ChainOutcome>,
    waiters: Vec<oneshot::Sender<Result<AcquiredLease, AcquireFault>>> }
```

This map is part of ControlPlane and shares its single mutation lock. Register
under that lock before exposing Establishing/Pending to another acquire. Settle,
store the result, wake waiters, and remove the key under the same lock. A waiter
that finds no entry after obtaining Pending retries acquire once; a completed
route then resolves as Reuse or typed rejection.
100 concurrent acquires -> one transport call (manager returns Pending,
waiters subscribe). Waiter cancellation drops the receiver; settle's sends
ignore closed channels (no corruption). Owner cancellation drops the acquire
future; the detached pipeline still completes the state machine (no permanent
Establishing). shutdown(): flag + resolve all waiters with Err(Shutdown).

Events (credential-free, evidence/debug only, never authorization):
AcquireStarted/Rejected, RoutePlanned, RoutePending, EstablishmentStarted/
Succeeded/Failed, ObservationRecorded, Verified/VerificationFailed,
RecoveryStarted, ReplacementPreserved/IdentityChanged, RouteExpired,
LeaseIssued, Shutdown.

Host-facing sync APIs: record_observation (generation-checked, stale stored as
history + typed error), state(route), events(), shutdown(). Expiry: leases
carry expires_at; enforcement at issuance and next acquire (documented M1
policy: no mid-use revocation).

## 7. FakeTransport (egress-runtime::fake)

```rust
pub enum ScriptedStep { Succeed, Fail(TransportFailure),
    AwaitRelease(Box<ScriptedStep>),          // deterministic external delay
    SucceedThenFail(TransportFailure) }       // establish-then-fail
pub struct FakeTransport { script, gate: tokio Notify, counters, session_failures }
```

- establish() consumes the next step; AwaitRelease parks on Notify until
  release_all(); no timers, no sleeping, deterministic ordering.
- SucceedThenFail resolves Established AND queues a session failure the test
  retrieves and feeds to runtime.record_observation (host-observed failure).
- Accessors: establishments(), established_keys(), waiting_keys(),
  pending_session_failures(). Test 21: same script -> identical event and
  observation sequence.
Observation and verification IDs are allocated monotonically under the CP lock.
Provider/operator observations without an attribution rule remain unattributed;
multiple distinct failure categories are attributed separately by the runtime
per establishment attempt, with corroboration only for the same category and
scope. Expired establishment is terminal. Replacement identity objects are
removed if planning rejects, and post-shutdown callbacks are historical only
and cannot mutate current state.

## 8. Test mapping (CRAFT-M1 numbers)

- m1_end_to_end.rs: 1,2,3,4,21
- m1_concurrency.rs: 5,6,7,8,22,23,24,25
- m1_staleness.rs: 9,10,11
- m1_replacement.rs: 12,13,14
- m1_recovery.rs: 15,16
- m1_expiry.rs: 17,18
- m1_observation.rs: 19,20
- m1_properties.rs: P1..P5 (proptest, current_thread runtime, no timers)

Properties:
- P1 stale callback ordering: random shuffled/duplicated stale completes and
  observations never mutate current-generation state.
- P2 arbitrary observations never create verification: EvidenceVerifier
  returns InsufficientEvidence / None properties unless matching
  EgressObserved evidence exists for the current key.
- P3 recovery never violates leakage: for random policies and failures,
  acquire either returns a lease backed by a real establishment or a typed
  error; no direct path exists in the type space at all.
- P4 at most one establishment per route generation across random
  release/cancel interleavings (fake records keys per call).
- P5 identity generation increments only on accepted continuity-breaking
  replacement (event/state correlation over random op sequences).

## 9. Open decisions resolved

- Detached pipeline task (not owner-driven inline) so owner cancellation
  cannot strand Establishing. Spec's "release mutation boundary" pins the
  lock-drop-before-await shape; task detachment is the cancellation-safe
  completion of that shape.
- std Mutex + Send futures over tokio Mutex: the spec phrase "manager mutation
  lock" presumes a synchronous lock; Send-bound makes violations non-compiling.
- ObservationSnapshot moves to observation-core (health projection ownership).
- Recovery depth bounded: RetrySameRoute bounded by attempts; Replan policies
  recover once per acquire chain; FailClosed zero. Documented, deterministic.
- VerifiedEgress stays constructible by hosts (verifier contract) but the
  forbidden conversions are compile-fail tested.

## 10. Deliberate exclusions (CRAFT-M1 honored)

No SOCKS5/CONNECT/MASQUE/QUIC/relay/DNS/provider APIs/geography services/
external verification/persistence/ML. Tokio confined to egress-runtime.
