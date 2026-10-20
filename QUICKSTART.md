# Egrissa quickstart

Use this guide from a fresh checkout to build the current CLI and run the repository quality gate. It is based on [`Cargo.toml`](Cargo.toml) and [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Prerequisites

- Git.
- Rust **1.85** and Cargo. The workspace declares `rust-version = "1.85"`; it does not include a tracked `rust-toolchain.toml`.
- The `rustfmt` and `clippy` Rust components.

## Clone, configure, and validate

```sh
git clone <repository-url> egrissa
cd egrissa

rustup toolchain install 1.85.0 --profile minimal
rustup component add --toolchain 1.85.0 rustfmt clippy
rustup override set 1.85.0

cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo package --workspace --list
```

The first three Cargo commands are the required workspace quality gate. CI additionally lists package contents. It intentionally does **not** run publish-style `cargo package` verification because the internal workspace crates are not published to crates.io yet.

## Run the current local command

```sh
cargo run --package egrissa-cli -- --help
cargo run --package egrissa-cli -- --version
```

The `egrissa` binary currently exposes help and version output only. It does not start a proxy or service, accept connection settings, or require runtime secrets; see [`crates/egrissa-cli/src/main.rs`](crates/egrissa-cli/src/main.rs).

## Configuration and limits

No tracked environment variable, database, external service, or local daemon is required for the commands above. [`.gitignore`](.gitignore) excludes `.env.local` and `.envrc`, but no repository-supplied environment loader is tracked; keep any local secret material out of Git.

For architecture, adapter boundaries, and contribution guidance, continue with [`ONBOARDING.md`](ONBOARDING.md).
