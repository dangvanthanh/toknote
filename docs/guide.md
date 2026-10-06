# Guide

How to build, run and install toknote from source.

## Toolchain

The project pins Rust **1.99.0** in `rust-toolchain.toml`; rustup picks it up automatically. SQLite is bundled through `rusqlite`, so no system library is needed. macOS or Linux.

If a version manager pins another toolchain (for example, mise sets `RUSTUP_TOOLCHAIN`), `cargo` fails with `requires rustc 1.99`. Override it for the current shell without changing global settings:

```sh
export RUSTUP_TOOLCHAIN=1.99.0
alias cargo=~/.cargo/bin/cargo   # bypass the version manager's shim
```

## Run without installing

Arguments after `--` go to toknote, not cargo.

```sh
cargo run --release                   # interactive TUI
cargo run --release -- week           # one-shot summary: today|week|month|7d|30d
cargo run --release -- 30d --json     # machine output
cargo run --release -- today --live   # fetch real account limits (network, opt-in)
```

## Build

| Command                                       | Output                     | Notes                     |
| --------------------------------------------- | -------------------------- | ------------------------- |
| `cargo build --release`                       | `target/release/toknote`   | default                   |
| `cargo build --profile small`                 | `target/small/toknote`     | smallest binary           |
| `cargo build --release --no-default-features` | `target/release/toknote`   | offline-only, no HTTP code |

## Install (optional)

```sh
cargo install --path .                                  # → ~/.cargo/bin/toknote
# or
mkdir -p ~/.local/bin
install -m755 target/release/toknote ~/.local/bin/toknote
```

## Usage

```
toknote                          # interactive TUI
toknote today|week|month|7d|30d  # one-shot summary
toknote ... --json               # machine output
toknote ... --live               # fetch real account limits (network, opt-in)
toknote ... --redact             # hide file paths in output
toknote ... --verbose            # report skipped (malformed) log lines
toknote --help | --version       # also -h / -V
```

TUI keys:

- `d` / `w` / `m`: switch the period.
- `↑` / `↓`: select a tool; `Enter`: show its per-model breakdown.
- `r`: rescan the logs.
- `l`: fetch live limits (asks first).
- `q`, `Esc` or `Ctrl-C`: quit.

## Environment

| Variable               | Default          | Used for                       |
| ---------------------- | ---------------- | ------------------------------ |
| `HOME`                 | home directory   | Claude Code and Command Code logs |
| `CODEX_HOME`           | `~/.codex`       | Codex sessions and login       |
| `XDG_DATA_HOME`        | `~/.local/share` | OpenCode database              |
| `XDG_CACHE_HOME`       | `~/.cache`       | live limits cache (`--live`)   |
| `COMMAND_CODE_API_KEY` | unset            | Command Code live limits       |

## Before committing changes

```sh
cargo fmt --check
cargo clippy --release --all-targets
cargo clippy --release --no-default-features
cargo build --release
cargo build --profile small --no-default-features
```

The full checklist, including manual checks, is in [AGENTS.md](../AGENTS.md#verify).
