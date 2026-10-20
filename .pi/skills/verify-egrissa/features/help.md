# Help

Help lets a user discover the Egrissa command and its supported version flag.

## Sub-features

- `help-default` shows help when the command has no argument.
- `help-explicit` shows the same usage via `--help`.
- `help-short` shows the same usage via `-h`.

## How to get to it (user POV)

- Run `egrissa` with no argument in a terminal.
- Run `egrissa --help` in a terminal.
- Run `egrissa -h` in a terminal.

## Driving it with shell

Preconditions:

- `cargo build --package egrissa-cli` has completed.
- `target/debug/egrissa --version` exits `0`.

- **Default help.** Run `target/debug/egrissa >"$EVIDENCE_DIR/help-default.stdout" 2>"$EVIDENCE_DIR/help-default.stderr"; echo $? >"$EVIDENCE_DIR/help-default.exit"`. Exit code is `0`; stdout contains `Usage: egrissa [--version]`.
- **Explicit help.** Run `target/debug/egrissa --help >"$EVIDENCE_DIR/help-explicit.stdout" 2>"$EVIDENCE_DIR/help-explicit.stderr"; echo $? >"$EVIDENCE_DIR/help-explicit.exit"`. Exit code is `0`; stdout contains the same usage string.
- **Short help.** Run `target/debug/egrissa -h >"$EVIDENCE_DIR/help-short.stdout" 2>"$EVIDENCE_DIR/help-short.stderr"; echo $? >"$EVIDENCE_DIR/help-short.exit"`. Exit code is `0`; stdout contains the same usage string.

## Gotchas

- `cargo run` adds compile and runner output; use the built binary for command evidence.
- `help` is also an accepted alias, but the three listed entry points are the required proof paths.
