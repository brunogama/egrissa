# Security
Trust boundaries: workload; control plane; credential store/provider adapter; transport; relay ingress/egress; upstream provider; destination; verifier.

Rules: secrets never enter domain events; fail closed by default; direct fallback explicit; generation-check delayed callbacks; multi-hop operator separation enforceable before allocation; verification independent from provider assertions; cancellation cannot leave permanent Establishing; shutdown resolves waiters; relay forwarding enforces destination/policy restrictions; logs redact credentials/sensitive fields.

Abuse boundary: this is generic egress infrastructure. Do not encode logic intended to defeat destination quotas, suspensions, authentication or access controls. Destination responses may be observed/classified; generic declared policy controls recovery.
