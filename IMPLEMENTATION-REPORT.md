# M7 implementation report

## Delivered behavior

`route-core::intelligence` ranks eligible route candidates from immutable typed facts. Reliability uses only endpoint, upstream, or transport scope. Latency, cooldown, and destination evidence retain observation IDs. Cost is an opaque caller-supplied hook.

The existing planner still owns hard eligibility. `DeterministicPlanner::plan_with_decision` applies those checks first, ranks the resulting candidates, and returns the chosen `RoutePlan` with a `RouteDecisionRecord`. The existing `RoutePlanner::plan` keeps its compatibility behavior.

Destination evidence applies only when its `DestinationKey` matches the request destination. It does not enter `ObservationSnapshot` or endpoint reliability. Active cooldowns appear as typed exclusions with their expiry and evidence.

`SelectionPolicy::Deterministic` is the default. `AdaptiveWithinEqualScore` uses the injected chooser only inside an equal-score group. Invalid chooser output records a deterministic fallback.

## Invariant to test matrix

| Invariant | Test |
| --- | --- |
| Deterministic inputs produce deterministic rankings | `input_permutation_does_not_change_deterministic_ranking` |
| Destination evidence does not poison other destinations | `destination_evidence_reorders_only_its_destination` |
| Cooldowns are explicit and evidence-bound | `active_cooldown_is_excluded_with_its_evidence` |
| Reliability, latency, and cost affect ranking | `reliability_outranks_static_rank`, `latency_and_cost_break_ties_after_reliability` |
| Adaptive selection has deterministic fallback | `invalid_adaptive_choice_falls_back_deterministically` |
| Hard eligibility remains in the planner | `planner_uses_intelligence_after_hard_eligibility` |

## Verification

Run the following commands from the repository root.

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
