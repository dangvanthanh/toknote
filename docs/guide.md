# Guide

How to build, run and install toknote from source. For commands and keys, see the [README](../README.md#usage).

## Toolchain

The project pins Rust **1.99.0** in `rust-toolchain.toml`; rustup picks it up automatically. SQLite is bundled through `rusqlite`, so no system library is needed. macOS or Linux.

If a version manager pins another toolchain (for example, mise sets `RUSTUP_TOOLCHAIN`), `cargo` fails with `requires rustc 1.99`. Override it for the current shell without changing global settings:

```sh
export RUSTUP_TOOLCHAIN=1.99.0
alias cargo=~/.cargo/bin/cargo   # bypass the version manager's shim
```

## Build

| Command                                       | Output                   | Notes                      |
| --------------------------------------------- | ------------------------ | -------------------------- |
| `cargo build --release`                       | `target/release/toknote` | default                    |
| `cargo build --profile small`                 | `target/small/toknote`   | smallest binary            |
| `cargo build --release --no-default-features` | `target/release/toknote` | offline-only, no HTTP code |

## Run without installing

Arguments after `--` go to toknote, not cargo.

```sh
cargo run --release                   # interactive TUI
cargo run --release -- week           # one-shot summary
cargo run --release -- 30d --json     # machine output
```

## Install

```sh
cargo install toknote                                   # from crates.io → ~/.cargo/bin/toknote
# or from a checkout
cargo install --path .
# or copy a built binary
mkdir -p ~/.local/bin
install -m755 target/release/toknote ~/.local/bin/toknote
```

## Environment

| Variable               | Default          | Used for                          |
| ---------------------- | ---------------- | --------------------------------- |
| `HOME`                 | home directory   | Claude Code and Command Code logs |
| `CODEX_HOME`           | `~/.codex`       | Codex sessions and login          |
| `XDG_DATA_HOME`        | `~/.local/share` | OpenCode database                 |
| `XDG_CACHE_HOME`       | `~/.cache`       | live limits cache (`--live`)      |
| `COMMAND_CODE_API_KEY` | unset            | Command Code live limits          |

A variable that is set but empty is used as is.

## Contributing

Rules and the full verification checklist are in [AGENTS.md](../AGENTS.md).
