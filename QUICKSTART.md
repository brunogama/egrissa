# Egrissa Quickstart

This guide validates a fresh checkout. It does not require credentials, a database, a network service, or an external provider.

## Prerequisites

- Rust **1.85** and Cargo. The workspace declares this MSRV in [`Cargo.toml`](Cargo.toml).
- Git.

## Clone and validate

```sh
git clone git@github.com:brunogama/egrissa.git
cd egrissa
rustup toolchain install 1.85.0 --profile minimal
rustup component add --toolchain 1.85.0 rustfmt clippy
rustup override set 1.85.0
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Run the local command

```sh
cargo run --package egrissa-cli -- --help
cargo run --package egrissa-cli -- --version
```

The current CLI is a distribution entry point with help and version commands only. It does not start a proxy, make network connections, require secrets, or configure a managed service. See [`crates/egrissa-cli/src/main.rs`](crates/egrissa-cli/src/main.rs).

## Optional local environment loading

No environment variables are required for the commands above. If a developer has a local `.env.local`, the ignored `.envrc` can load it when sourced:

```sh
. ./.envrc
```

Do not commit `.env.local` or `.envrc`; both are ignored by [`.gitignore`](.gitignore).
