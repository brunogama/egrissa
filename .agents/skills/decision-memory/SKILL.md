---
name: decision-memory
description: Decision ledger for recording durable user choices and recalling concise relevant summaries. Use when a user selects a direction, sets a project constraint, approves or rejects an approach, or when planned work may be constrained by earlier user decisions.
---

# Decision Ledger

Use this ledger for durable user choices that affect future repository work. It is not a task log, an implementation diary, a secret store, or a replacement for `docs/adr/`.

The canonical store is `.pi/decisions.jsonl`. Use the deterministic helper at `.agents/skills/decision-memory/scripts/decisions.py`.

## Recall

1. Before choosing an approach or changing behavior with uncertain history, query the ledger using the task's important nouns:

   ```sh
   python3 .agents/skills/decision-memory/scripts/decisions.py query "<task terms>"
   ```

2. Treat every returned active entry as a constraint or preference. Read any linked source it names.
3. Search `docs/adr/` separately for architectural changes, invariants, ownership, or dependency policy; ADRs remain authoritative there.

Recall is complete when the plan follows every relevant active decision or explains why a newer documented decision supersedes it.

## Record

1. Record only explicit user choices likely to matter after this conversation: a selected option, stated policy, approval/rejection, or durable constraint.
2. Capture the choice and rationale in the user's terms. Add narrow scope and stable tags. Exclude credentials, private personal data, transient task status, and unapproved agent preferences.
3. Append through the helper, using a stable interaction key when available:

   ```sh
   python3 .agents/skills/decision-memory/scripts/decisions.py record \
     --scope "<affected subsystem>" \
     --decision "<what the user chose>" \
     --rationale "<why they chose it>" \
     --tag "<topic>,<topic>" \
     --key "<stable source-message identifier>"
   ```

4. When a newer user choice replaces an earlier one, append it with `--supersedes D-####`; preserve history.

Record is complete when the helper prints `recorded D-####` or `unchanged D-####`, and a targeted query returns the expected concise summary.

## Return

Return only the relevant helper summary to an agent query or user report. Include the decision ID, choice, and rationale; omit unrelated history. Use `--json` only when a caller requires structured fields.

## Escalate

For a repository-wide architecture, invariant, security boundary, or dependency-policy choice, create or update the appropriate ADR rather than duplicating its full rationale here. The ADR becomes authoritative; retain only a short ledger pointer when it improves recall.

## Verify

```sh
STORE="$(mktemp)"
python3 .agents/skills/decision-memory/scripts/decisions.py --store "$STORE" record \
  --scope "skill verification" \
  --decision "Use the decision ledger" \
  --rationale "Future agents need durable user choices" \
  --tag "agents" \
  --key "verification-1" \
  --at "2026-01-01T00:00:00Z"
python3 .agents/skills/decision-memory/scripts/decisions.py --store "$STORE" query "durable choices"
rm -f "$STORE" "$STORE.lock"
```

Verification is complete when recording is idempotent for its key and the query prints the decision, scope, and rationale.
