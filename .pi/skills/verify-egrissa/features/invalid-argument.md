# Invalid argument

Invalid argument handling gives a user immediate, actionable feedback instead of silently accepting unsupported input.

## Sub-features

- `invalid-rejected` rejects an unknown positional argument.
- `invalid-guidance` tells the user to run the documented help command.

## How to get to it (user POV)

- Run `egrissa unknown` in a terminal.

## Driving it with shell

Preconditions:

- `cargo build --package egrissa-cli` has completed.
- `target/debug/egrissa --version` exits `0`.

- **Reject unknown input.** Run `set +e; target/debug/egrissa unknown >"$EVIDENCE_DIR/invalid.stdout" 2>"$EVIDENCE_DIR/invalid.stderr"; code=$?; set -e; printf '%s\n' "$code" >"$EVIDENCE_DIR/invalid.exit"`. The recorded exit code is `2`; stderr contains `unknown argument: unknown` and `Run \`egrissa --help\` for usage.`; stdout is empty.

## Gotchas

- The expected nonzero exit is product behavior, not a harness failure; capture it with `set +e` and then restore `set -e`.
- Assert both the exit code and stderr guidance. Either one alone is incomplete feedback proof.
