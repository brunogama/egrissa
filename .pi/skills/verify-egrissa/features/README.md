# Egrissa CLI verification map

This map is the maintained source for user-facing verification of the Egrissa distribution CLI. Read the index, build the binary, run the Doctor command in [`../SKILL.md`](../SKILL.md), then drive the relevant feature in a fresh process.

## Baseline preconditions

- Work from the repository root.
- `cargo build --package egrissa-cli` succeeds.
- `target/debug/egrissa --version` exits `0`.
- Create a new evidence directory under `.pi/skills/verify-egrissa/artifacts/` for the run.
- Do not assume an npm script exists: this is a Rust workspace and has no `package.json`.

## Driving conventions

- Treat each command as literal and run it in a fresh process.
- Capture command, stdout, stderr, and exit code for every path.
- Use the built `target/debug/egrissa` binary after the initial Cargo build, avoiding compilation output in command-specific evidence.
- A help/version output alone does not verify invalid input; verify the documented exit code and stderr separately.

## Proof and skip reporting

- Record the feature ID, command, exit code, stdout, and stderr in the evidence directory.
- Do not call internal Rust functions or test-only paths as user-path proof.
- Report an unreachable feature with the attempted command and unmet prerequisite.
- Preserve terminal proof after cleanup.

## Features

- [Help](./help.md) — default and explicit help entry points.
- [Version](./version.md) — installed binary version output.
- [Invalid argument](./invalid-argument.md) — fail-fast feedback and exit status.
