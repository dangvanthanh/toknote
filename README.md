# Tok Note

Privacy-first terminal tool that reads local logs and shows tokens, estimated cost, and 5h / weekly usage limits with reset times.

- [Claude Code](https://claude.com/claude-code)
- [Codex](https://openai.com/codex)
- [Command Code](https://commandcode.ai/)
- [OpenCode](https://opencode.ai/)

Only tools with local data are shown.

```
 ▀█▀ █▄ █  v0.1.0                              Today  [Week]  Month
  █  █ ▀█  ↑↓ select · ⏎ models · d/w/m period · r refresh · q quit

  $48.20 est · 9.4M tokens · Oct 1–5

 › Claude   $31.10   6.2M
     5h  ██░░░░░░░░░░  20% used  resets in 1h 12m · 17:05
     7d  ██████░░░░░░  47% used  resets Tue 08:00

   OpenAI   $17.10   3.2M
     5h  ██████████░░  82% used  resets in 12m · 16:05
     7d  ██████░░░░░░  53% used  resets Mon 08:00

  limits: live · 2m ago · l off
```

## Install

macOS or Linux, Rust 1.99.0 or newer:

```sh
cargo install toknote
# or from a checkout (uses the toolchain pinned in rust-toolchain.toml)
cargo install --path .
```

The [guide](docs/guide.md) covers build profiles, an offline-only build, running from source, toolchain overrides and environment variables.

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

- Periods use your local time zone. `today`, `week` (from Monday) and `month` are calendar periods; `7d` / `30d` are rolling.
- TUI keys: `d/w/m` period, `↑/↓` select a tool, `Enter` per-model breakdown, `r` rescan logs, `l` live limits (asks first), `q` / `Esc` / `Ctrl-C` quit.
- Limits always show **percent used** (JSON/cache `used_pct`). Bars are green below 50% used, yellow from 50%, red from 80%.
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

- No network unless you pass `--live` (or confirm `l` in the TUI). A `--no-default-features` build contains no network code at all.
- Reads usage fields only, never message content. OpenCode's database is opened read-only.
- No telemetry. Prices are compiled into the binary.
- The only file toknote writes is `~/.cache/toknote/live.json` (mode 0600, limits only), and only with `--live`.

## Known limitations

- Tokens and cost cover local sessions only. Codex cloud tasks and usage on other machines leave no local token data ([details](docs/models/openai.md#cloud-tasks)).
- Limits are account-wide, so `--live` percentages can be high while local tokens are low.
- Offline OpenAI limits come from the newest Codex log and show `0% used` once their reset has passed, even if Codex hasn't run since. Use `--live` for a current reading.
- Claude and Command Code offline limits are estimates: reset times only, no percent.
- Cost is an API-equivalent estimate (`est.`); subscription users don't pay it. Models without a published price show `?`. Command Code and OpenCode costs are the tools' own recorded estimates; OpenCode free and subscription models record `$0.00`.
- Command Code support was built from the CLI's bundled code and isn't yet verified against real session files ([details](docs/models/commandcode.md)).
- OpenCode: no usage limits (OpenCode Go caps have no public endpoint), and only `opencode.db` is read, not the legacy JSON store ([details](docs/models/opencode.md)).
- The TUI clips long lines instead of wrapping: the key hints below about 68 columns, the empty-state message below about 85.

## Docs

- [Guide](docs/guide.md): build, run, install, environment
- [Architecture](docs/architecture.md): internals
- Data formats: [Claude Code](docs/models/claude.md), [OpenAI (Codex)](docs/models/openai.md), [Command Code](docs/models/commandcode.md), [OpenCode](docs/models/opencode.md)
- [AGENTS.md](AGENTS.md): contributor rules and verification

## License

MIT
