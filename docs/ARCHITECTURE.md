# Architecture
```text
Workload -> EgressRequest -> Policy/Capabilities -> NetworkIdentity -> RoutePlan
        -> Establishment -> Observation -> Attribution -> Verification -> Lease/Recovery
```
Planes: control (request/identity/policy/planning), data (transport/provider adapters), evidence (observations/health/attribution/verification).

Target crates: `egress-core`, `identity-core`, `route-core`, `observation-core`, `verification-core`, `transport-core`, `egress-runtime`, `transport-socks5`, `transport-http-connect`, `transport-relay`, `transport-masque`, `provider-core`, `relay-client`, `relay-server`, `local-proxy`, `egress-cli`.

Domain crates never depend on concrete transports. State strength is one-way: desired < allocated < established < observed < verified.
