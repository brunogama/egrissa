# Invariants
1. Desired != allocated. 2. Allocation != establishment. 3. Establishment != observation. 4. Observation != verification.
5. Network identity != IP. 6. Unknown continuity stays unknown without evidence.
7. Route/identity generations are independent. 8. Stale feedback cannot mutate current state.
9. Domain holds credential references only. 10. `ProhibitDirect` cannot silently produce direct connectivity.
11. Destination failures do not automatically poison endpoint health. 12. Provider claims are not verification.
13. Unsupported topology rejected before establishment. 14. Operator separation checked before allocation.
15. Identity-changing recovery explicit. 16. Route validity <= every dependency validity.
17. One establishment owner per route generation. 18. Compatible concurrent acquire does not duplicate allocation.
19. No manager mutation lock across network await. 20. Planning deterministic unless randomness explicitly injected.
21. Raw provider errors sanitized at adapters. 22. Health is projection over immutable evidence.
23. Expiration rejects new use but does not inherently kill active streams. 24. Failure to observe != evidence of absence.
25. Application status codes do not directly control topology.
