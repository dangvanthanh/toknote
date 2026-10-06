# AGENTS.md

Rust CLI/TUI reading local Claude Code, Codex, Command Code and OpenCode usage. Read [docs/architecture.md](docs/architecture.md) before changing code, and the matching [docs/models/](docs/models/) file before touching a source or pricing.

## Rules

- **No unit tests.** Do not create `#[test]` / `#[cfg(test)]` code, `tests/` files, fixtures, dev-dependencies or scripts. Verify with builds, formatting, clippy and focused manual checks.
- **No commits** unless the user explicitly asks.
- Rust **1.99.0** (`rust-toolchain.toml`), edition 2024. Dependencies are limited to: `clap`, `serde` / `serde_json`, `chrono`, `dirs`, `walkdir`, `memchr`, `rusqlite` (bundled SQLite), `ratatui` + `crossterm`, `libc`, and `ureq` (live only). Don't add others without asking.
- Small focused modules; prefer the standard library over custom implementations.
- **Privacy invariants:**
  - No network outside `src/live.rs`; only with `--live` or confirmed TUI `l`.
  - `cargo build --no-default-features` compiles without HTTP code (`ureq` is optional behind the `live` feature).
  - Pre-filter JSONL lines by exact usage/kind substrings; deserialize usage-only serde structs (unknown fields are skipped). Never declare message content fields.
  - Open SQLite read-only, select usage fields with `json_extract`; never read `part`.
  - Never print credentials or response bodies. Reject CR/LF in credential headers; do not follow redirects; ignore environment proxies.
  - Runtime writes are limited to the live cache: limits only, directory 0700, file 0600.
- Costs are labeled `est.`. Unknown prices show `?`, never a guessed price.
- Microsecond UTC timestamps internally; chrono `Local` for calendar periods (ambiguous midnights take the earlier instant, DST-skipped midnights move forward).
- Displayed numbers use `model::fixed` (shortest decimal, rounded half up), not `{:.N}`.
- Restore terminal state on normal exit, errors, panic and common termination signals (`tui::term::Guard`).
- SQLite is used from one thread: its one scan job opens, queries and closes the database.
- JSONL sources expose `files` (sequential discovery) and `parse` (one file, on a worker thread). `parse` returns owned results and never touches the shared `Scan`; `output::load` merges results in discovery order.
- The version lives only in `Cargo.toml`; `--version`, the TUI header and the live User-Agent read it via `features::VERSION`.

## Verify

```sh
cargo fmt --check
cargo clippy --release --all-targets
cargo clippy --release --no-default-features
cargo build --release
cargo build --profile small --no-default-features
cargo build --profile small
# Cross-compilation check (execution on Linux is still a separate manual check):
cargo build --release --target x86_64-unknown-linux-gnu
```

If a version manager pins another toolchain (e.g. mise sets `RUSTUP_TOOLCHAIN`), run with `RUSTUP_TOOLCHAIN=1.99.0 ~/.cargo/bin/cargo ...` without modifying global settings.

Focused manual checks:
- Claude totals vs independent usage-only `jq` sum (dedup `message.id + requestId`; exclude `<synthetic>`).
- Codex active + archived sessions: repeated cumulative totals and duplicate rollout filenames count once.
- Command Code copies and sidecars: dedup replies; recorded costs override pricing.
- OpenCode totals vs usage-only `sqlite3` sums on a private `.backup`, never mutate the real database.
- Pipe safety: `target/release/toknote 30d | head -1` must not panic.
- Empty state: with a private empty HOME and explicit empty CODEX_HOME / XDG_DATA_HOME, prints `No usage found in ~/.claude, ~/.codex, ~/.commandcode or ~/.local/share/opencode`.
- Redaction: `target/release/toknote today --verbose --redact` emits no home paths.
- Offline fallback on macOS: `sandbox-exec -p '(version 1)(allow default)(deny network*)' target/release/toknote today --live`.
- Cache fresh (<60s), stale fallback (<15m), expiry, 0700/0600 permissions.
- TUI: `d/w/m`, arrows, Enter, `r`, live confirmation/cancellation, `q`, Esc, Ctrl-C, SIGTERM/SIGHUP, resize. Header at 72 / 60 / 48 columns. Check terminal restoration.
- Parallel scan: repeated `--json` runs are byte-identical, and output matches the previous build on a private log snapshot.
- Never copy credentials to a log snapshot, print message contents, or modify real logs/cache for verification.

## Pricing and docs

- Prices: `src/pricing.rs` (USD per 1M tokens). Update from its official sources, bump the date, mirror in model docs.
- Command Code / OpenCode recorded costs override the table.
- `README.md`: usage, privacy, limitations.
- `docs/architecture.md`: modules, data flow, limits, cache.
- `docs/models/{claude,openai,commandcode,opencode}.md`: formats and endpoints (`openai.md` covers Codex).
