# Tok Note

Privacy-first terminal tool that reads local, logs and shows tokens, estimated cost, and 5h / weekly usage limits with reset times.

- [Claude Code](https://claude.com/claude-code)
- [Codex](https://openai.com/codex)
- [Command Code](https://commandcode.ai/)
- [OpenCode](https://opencode.ai/)

Only tools with local data are shown.

```
 ▀█▀ █▄ █  v0.1.0                              Today  [Week]  Month
  █  █ ▀█  ↑↓ select · ⏎ models · d/w/m period · r refresh · q quit

  $48.20 est · 9.4M tokens · Oct 1–5

 › Claude   $31.10   5.2M
     5h  ██░░░░░░░░░░  20% used  resets in 1h 12m · 17:05
     7d  ██████░░░░░░  47% used  resets Tue 08:00

   OpenAI   $17.10   3.2M
     5h  ██████████░░  82% used  resets in 12m · 16:05
     7d  ██████░░░░░░  53% used  resets Mon 08:00

  limits: live · 2m ago · l off
```

## Install

```sh
# Rust 1.99.0 (pinned in rust-toolchain.toml); macOS or Linux. SQLite is bundled.
cargo build --release            # or --profile small for the smallest binary
# Optional install:
mkdir -p ~/.local/bin
install -m755 target/release/toknote ~/.local/bin/toknote
# or: cargo install --path .

# Offline-only binary with no network code at all:
cargo build --release --no-default-features
```

See the [guide](docs/guide.md) for running from source, toolchain overrides and environment variables.

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

```
$ toknote week
Week  $48.20 est · 9.4M tok
Claude $31.10 6.2M   5h ↻14:30 est
OpenAI $17.10 3.2M   5h 82% used ↻16:05 · 7d 53% used ↻Mon 08:00
```

- Periods use your local timezone. Weeks start Monday. `week`/`month` are calendar periods; `7d`/`30d` are rolling.
- TUI keys: `d/w/m` period, `↑/↓` + `enter` per-model breakdown, `r` refresh, `l` live (asks first), `q` / `Esc` / `Ctrl-C` quit. The header shows the version and these keys; the key line is cut off in terminals narrower than about 68 columns.
- Every tool shows **percent used**, matching JSON/cache `used_pct`. Bars use soft pastel colors: green below 50% used, yellow from 50%, red from 80%.
- Reset times within a day are relative and refresh every 30s.

## Limits

| Tool         | Offline (default)                      | Label  |
| ------------ | -------------------------------------- | ------ |
| OpenAI       | Codex logs: percent used + reset time  | `log`  |
| Claude       | 5h window reset time, no percent       | `est`  |
| Command Code | 5h + 7d window reset times, no percent | `est`  |
| OpenCode     | none (no usage limits)                 |        |
| First three  | account usage via `--live`             | `live` |

`--live` uses your existing Claude Code / Codex / Command Code logins and skips tools you aren't logged in to. On failure it falls back to offline data. Results are cached for 60s (and reused up to 15 min on errors) to avoid HTTP 429.

## Privacy

- No network unless you pass `--live` (or confirm `l` in the TUI).
- Reads usage fields only, never message content.
- No telemetry. Prices are compiled into the binary.
- OpenCode's SQLite database is opened read-only, and only usage fields are queried.
- The only file toknote writes is `~/.cache/toknote/live.json` (mode 0600, limits only), and only with `--live`.

## Known limitations

- Tokens and cost cover local sessions only. Codex cloud tasks (ChatGPT-hosted) and usage on other machines leave no local token data.
- Limits are account-wide, so `--live` Codex percentages can be high while local tokens are low. This is expected.
- Offline OpenAI limits come from the newest Codex log and assume `0% used` after a reset if Codex hasn't run locally since. This is not a current account reading; use `--live`.
- Claude offline limits are an estimate: 5h reset time only, no percent or weekly window.
- Command Code offline limits are estimates (reset times only). Its cost is Command Code's own recorded estimate; credit plans bill in credits. Command Code support was built from the CLI's bundled code and isn't yet verified against real session files.
- OpenCode: free and subscription models record `$0.00`. No usage limits are shown (OpenCode Go caps have no public endpoint). Only `opencode.db` is read, not the legacy JSON store.
- Cost is the API-equivalent estimate (`est.`). Subscription users don't pay it. Models without a published price show `?`.

## Docs

- [Guide](docs/guide.md): build, run, install
- [Architecture](docs/architecture.md)
- [Claude Code data](docs/models/claude.md)
- [OpenAI (Codex) data](docs/models/openai.md)
- [Command Code data](docs/models/commandcode.md)
- [OpenCode data](docs/models/opencode.md)

## License

MIT
