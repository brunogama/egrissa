---
name: verify-egrissa
description: Verify the Egrissa command-line distribution entry point: build it locally, exercise help, version, and invalid-argument paths, and retain terminal evidence. Use when changing `crates/egrissa-cli` or its documented CLI behavior.
---

# Verify Egrissa CLI

Egrissa currently exposes a short-lived CLI only: help, version, and invalid-argument handling. It does not start a proxy, make network connections, require credentials, or use a shared service.

## Launch

Build once from the repository root:

```sh
cargo build --package egrissa-cli
```

The CLI is ready when `target/debug/egrissa --version` exits `0` and prints the workspace version. It is a short-lived binary, so each drive runs a fresh process; do not launch a server or retain a PTY session.

## Doctor

Before each drive, run:

```sh
target/debug/egrissa --version
```

Drive only when this exits `0` with a version string. If the binary is absent or stale, repeat **Launch**. No ports, data directories, credentials, or authentication state are involved.

## Drive

Create a unique evidence directory before driving:

```sh
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR=".pi/skills/verify-egrissa/artifacts/$RUN_ID"
mkdir -p "$EVIDENCE_DIR"
```

Use the matching feature-map recipe in [`features/README.md`](features/README.md). Capture each command's stdout, stderr, and exit code. The stable command handles are:

- `target/debug/egrissa --help`
- `target/debug/egrissa --version`
- `target/debug/egrissa <unknown-argument>`

The first two must exit `0`; an unknown argument must exit `2`, print the unknown value to stderr, and direct the user to `egrissa --help`.

## Evidence

Proof is a terminal transcript: command, stdout, stderr, and exit code saved under the run-specific `artifacts/$RUN_ID/` directory. Exercise the real binary; do not call `main` or test-only internals. The feature recipe must capture both the action and its observable output. This CLI has no persistent side effect, file output, network call, or external boundary to inspect.

## Cleanup

Each CLI process exits on its own. Remove no evidence directory. If a command is interrupted, record the interruption in its transcript and remove only a process explicitly started by this run, using its recorded PID; never kill by process name. No shared process, port, or scratch state exists to clean up.

## Helpers

This skill ships no helper scripts. The shell commands above and in the feature map are the complete harness.
