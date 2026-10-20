# Testing
Layers: unit → property → deterministic fake-runtime integration → adapter contract → protocol interoperability → cancellation/failure/chaos → local E2E.

Mandatory properties: stale generations cannot mutate current; lifetime bounds hold; distinct-operator never selects same operator; missing capability never plans; fail-closed never direct-falls-back; continuity-breaking replacement never reports preserved; observations cannot manufacture verification; one establishment owner/generation; deterministic inputs rank deterministically.

Inject clocks; avoid sleeps. Final report must contain invariant→test matrix.
