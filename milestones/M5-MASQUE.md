# M5-MASQUE
Implement CONNECT-UDP per RFC 9298 and CONNECT-IP per RFC 9484 when required. Support HTTP/3 and explicitly capability-gated HTTP/2 fallback. MASQUE must not leak into canonical domain semantics. Add downgrade/fail-closed/interoperability tests.

Gate: fmt, clippy -D warnings, full workspace tests, plus milestone-specific integration/property tests.
