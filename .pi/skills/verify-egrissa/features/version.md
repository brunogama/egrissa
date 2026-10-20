# Version

Version lets a user identify the version of the Egrissa binary they are running.

## Sub-features

- `version-long` prints the version through `--version`.
- `version-short` prints the version through `-V`.
- `version-word` prints the version through `version`.

## How to get to it (user POV)

- Run `egrissa --version` in a terminal.
- Run `egrissa -V` in a terminal.
- Run `egrissa version` in a terminal.

## Driving it with shell

Preconditions:

- `cargo build --package egrissa-cli` has completed.
- `target/debug/egrissa --version` exits `0`.

- **Long flag.** Run `target/debug/egrissa --version >"$EVIDENCE_DIR/version-long.stdout" 2>"$EVIDENCE_DIR/version-long.stderr"; echo $? >"$EVIDENCE_DIR/version-long.exit"`. Exit code is `0`; stdout is the Cargo package version followed by a newline.
- **Short flag.** Run `target/debug/egrissa -V >"$EVIDENCE_DIR/version-short.stdout" 2>"$EVIDENCE_DIR/version-short.stderr"; echo $? >"$EVIDENCE_DIR/version-short.exit"`. Exit code is `0`; stdout matches the long-flag output.
- **Word command.** Run `target/debug/egrissa version >"$EVIDENCE_DIR/version-word.stdout" 2>"$EVIDENCE_DIR/version-word.stderr"; echo $? >"$EVIDENCE_DIR/version-word.exit"`. Exit code is `0`; stdout matches the long-flag output.

## Gotchas

- The version comes from Cargo package metadata; do not hard-code `0.1.0` in a durable proof.
- Version output intentionally has no product-name prefix.
