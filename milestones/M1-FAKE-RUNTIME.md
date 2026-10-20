# M1-FAKE-RUNTIME
Add observation-core, verification-core, transport-core, egress-runtime and deterministic FakeTransport. Prove async establishment, Pending fan-out, leases, observation, verification, attribution, recovery, expiry, cancellation, shutdown, stale callbacks. Test 100 concurrent compatible acquires => one establishment.

Gate: fmt, clippy -D warnings, full workspace tests, plus milestone-specific integration/property tests.
