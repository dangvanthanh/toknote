# Architecture

## Data flow

```
sources::claude      ─┐                     ┌─> output::text / json
sources::openai      ─┤                     │
sources::commandcode ─┼─> Scan ─> aggregate ┤
sources::opencode    ─┘   (events, limits)  └─> tui::ui::draw
                               ▲
live (opt-in) ─────────────────┘ replaces a tool's offline limits
```

1. `output::load` collects files from each JSONL source (`files`), parses them in parallel (`parse`), and merges the results into one `Scan` of `Event`s and `Limit`s. OpenCode is read from SQLite (`scan`).
2. `aggregate::estimates` adds estimated windows: Claude 5h, Command Code 5h and 7d.
3. `--live` replaces each tool's offline limits with live ones when the fetch succeeds.
4. `aggregate::summarize` groups events by period, tool and model and applies `pricing`.
5. The one-shot output and the TUI render the same `Summary` + `Vec<Limit>`. They show only `Scan::visible`: tools whose log directory exists, or that have limits (e.g. from `--live`). JSON always lists every tool.

Logs are rescanned on every run; there is no log cache. `output::load` scans in parallel:

- Discovery is sequential: file lists (`walkdir`, depth-first in directory order, symlinks inside the tree not followed), the Codex file-name dedup, and the `found` flags.
- One job per file, plus one for OpenCode's SQLite, goes into a list that `available_parallelism` scoped threads (`std::thread::scope`) pull from through an atomic index, largest file first. The single SQLite job opens, queries and closes the database, so SQLite stays on one thread.
- Each file streams through a reused 1 MiB buffer, grown only for longer lines (1 GiB cap). Matching lines are found with `memchr::memmem` (SIMD) over the whole buffer (`files::FileLines`), and only those lines are deserialized with `serde_json` into usage-only structs. Only usage events and dedup keys persist.
- Results merge in discovery order, so cross-file dedup and the newest Codex limits are identical to a sequential scan.

On a ~450 MB local log set this takes about 0.03 s in both the `release` and `small` profiles.

## Modules

```
Cargo.toml         # version (single source), features, dependencies, profiles
rust-toolchain.toml
src/
  main.rs          # args, one-shot/TUI dispatch
  cli.rs           # clap arguments and periods; fixed help text
  ctx.rs           # environment paths
  model.rs         # Tool, Event, Limit, Scan, LiveResult, number formatting
  time.rs          # microsecond UTC timestamps, chrono local calendar, RFC 3339
  pricing.rs       # static pricing, recorded-cost override
  aggregate.rs     # ranges, model rows, totals, window estimates
  output.rs        # parallel source loading, text/JSON, skipped-line diagnostics
  json.rs          # serde output shapes and number formatting
  features.rs      # compile-time live switch, version from Cargo.toml
  live.rs          # opt-in HTTP (ureq), credentials, limits-only cache; `live` feature only
  sources/
    files.rs       # discovery, streaming line reader, memmem search, usage-only parse, ordered merge
    claude.rs
    openai.rs      # Codex rollouts
    commandcode.rs
    opencode.rs    # rusqlite, read-only
  tui/
    term.rs        # raw mode guard, signal/panic restoration, key decoding
    app.rs         # state and event loop
    ui.rs          # styled ratatui lines
```

Dependencies: `clap` (arguments), `serde` / `serde_json` (JSONL, output, cache), `chrono` (calendar), `dirs` (home), `walkdir` (discovery), `memchr` (substring search), `rusqlite` with bundled SQLite (OpenCode), `ratatui` + `crossterm` (TUI), `libc` (signal-safe terminal restoration), and `ureq` with rustls (live only).

## Data model

```rust
enum Tool { Claude, Openai, Commandcode, Opencode } // Openai = Codex
struct Event {
    tool: Tool, ts: i64, model: String,
    input: u64, output: u64, cache_read: u64,
    cache_write_5m: u64, cache_write_1h: u64,
    cost_usd: Option<f64>,
}
enum Window { FiveHour, Week } // JSON: "5h", "7d"
enum Origin { Log, Est, Live }
struct Limit {
    tool: Tool, window: Window, origin: Origin,
    used_pct: Option<f32>, resets_at: Option<i64>,
    block_tokens: Option<u64>, fetched_at: Option<i64>,
}
```

Times are UTC epoch microseconds. JSON emits RFC 3339, with 0/3/6 fractional digits. Local calendar boundaries use chrono's `Local` (the system time zone, `TZ` or `/etc/localtime`). Ambiguous local midnights take the earlier instant, and a midnight skipped by DST moves forward, as `mktime` with `tm_isdst = -1` does. No time zone database is vendored.

`input` excludes cached tokens, so the five counters sum to the total with no overlap.

`Scan.found: [bool; 4]` marks existing tool log directories (or OpenCode database). `Scan::visible(tool)` also considers tools with limits.

Displayed numbers round the shortest round-trip decimal half up (`model::fixed`: `2.15M` → `2.2M`, `82.5%` → `83%`). JSON floats are the shortest round-trip decimal, always with a fraction (`1.0`).

## Periods

- `today`: local midnight → now
- `week`: Monday 00:00 local → now
- `month`: 1st 00:00 local → now
- `7d` / `30d`: rolling, now − N days → now

Rows group by `(tool, model)`, and model suffixes like `[1m]` are kept as separate rows. Zero-token events are dropped. Model rows sort by tokens descending, then model name for deterministic ties. Token sums saturate at `u64::MAX` instead of overflowing on corrupt counters.

## Pricing

A cost recorded in the log (`Event.cost_usd`, from Command Code and OpenCode) is used as is. Otherwise `pricing.rs` holds USD per 1M tokens for input, output, cache read, 5m write and 1h write. Model names are normalized before lookup: a `[...]` suffix and a trailing `-YYYYMMDD` are stripped. Unknown models get `cost_usd: null`. A row with any unpriced event is unpriced. Totals sum the priced rows and report `unpriced_tokens`, shown as `$X+?`.

## Limits

| Origin | Source                                                   |
| ------ | -------------------------------------------------------- |
| `log`  | Codex: newest `rate_limits` in local rollouts            |
| `est`  | Claude 5h, Command Code 5h + 7d, from local timestamps   |
| `live` | Claude / Codex / Command Code usage endpoints (`--live`) |

- `used_pct` always means percent consumed, and every tool displays it as `% used`, with bars filling by consumption. Bar colors (green below 50%, yellow 50–79%, red 80% and up) and `limit reached` (100%) use the same value. The tool identifier is `openai`.
- A logged/cached window whose reset has passed becomes `used_pct: 0` with no reset time (displayed as `0% used`). This is a stale-data assumption, not proof of current account usage; use `--live`.
- `aggregate::estimates(scan, now)`: walk the tool's events sorted by time. A window starts at the first event at or after the previous window's end and lasts 5h or 7d. If the last window hasn't ended, show its reset time and tokens. This matches how both Claude and Command Code open windows on the first request.

## Live mode

- Only `src/live.rs` touches the network. Building without the default `live` feature (`--no-default-features`) removes it and `ureq` entirely.
- Each request is one `ureq` GET with a 5s global timeout. Credentials are sent only to fixed HTTPS endpoints (`https_only`); redirects are never followed (`max_redirects(0)`; a redirect is reported as `HTTP 3xx`), environment proxies are ignored, and CR/LF header values are rejected. Error bodies are never read; success bodies are capped at 1 MiB.
- On macOS the Keychain is read by running `security` with a 5s timeout; its output is never printed.
- OpenCode has no limits and is never fetched.
- Each tool is fetched independently. A failure only affects that tool. A tool with no local login (no credentials found) is skipped silently and not cached.
- Cache: `$XDG_CACHE_HOME/toknote/live.json` (default `~/.cache`), dir 0700, file 0600, per tool `{fetched_at, limits}`.
  - Under 60s old: reused, no request.
  - On failure: reused if under 15 min old, labeled `live Nm ago`, with a note.
  - Cached windows whose reset has passed have `used_pct: 0`.
  - Writes use an exclusive per-process temporary file (`create_new`, mode 0600) and atomic rename; existing cache-directory permissions are restricted to 0700.
- 401/403 suggests re-login. 429 shows `Retry-After` when present.

## TUI

- Built with `ratatui` on the `crossterm` backend. Each frame is a list of styled lines rendered as one `Paragraph`; ratatui diffs frames, so redraws don't flicker, and clips lines to the terminal width and height.
- Colors come from one soft pastel truecolor palette in `tui/ui.rs`: blue `#89b4fa`, green `#a6e3a1`, yellow `#f9e2af`, red `#f38ba8`. Blue (bold) marks the selection, the active period tab and the period total; yellow is used for notes; usage bars use green / yellow / red.
- Header: a two-row "TN" logo (T blue, N yellow). Row one has the version (from `Cargo.toml`) and the period tabs, right-aligned; row two has the key hints.
- Keys: `d/w/m` period, `↑/↓` select, `Enter` per-model rows, `r` rescan logs, `l` live, `q` / `Esc` / `Ctrl-C` quit.
- `l` asks `Fetch limits from Anthropic, OpenAI and Command Code? y/n` once per session. The prompt appears at the bottom; any other key cancels it, and `Ctrl-C` quits.
- Status line: live limits show the oldest live age once, `limits: live · 2m ago · l off`. Mixed sources list only local fallbacks: `limits: live · 2m ago · local: Claude · l off`. OpenCode is excluded because it has no limits. Offline: `limits: local logs · press l for live` (no hint in offline-only builds).
- Redraws on each key, on resize (crossterm resize events), and every 30s so relative reset times stay current. Logs are rescanned only on `r`. Model names and notes have control characters replaced before display.
- Raw mode, the alternate screen and the cursor are restored by a drop guard on normal exit and errors, by a panic hook, and on `SIGINT`, `SIGTERM` and `SIGHUP` by an async-signal-safe handler that restores the saved termios and exits with `128 + signal`.

## Errors

- Malformed lines that pass the substring pre-filter are skipped and counted per file (`--verbose`). With `--redact`, the path prints as `claude`, `openai`, `commandcode` or `opencode`. An unreadable OpenCode database counts as one skipped entry.
- Missing log directories aren't errors; that tool is hidden. If no tool has logs, toknote shows the empty state.
- Stdout write errors (closed pipe) are ignored; Rust already ignores `SIGPIPE`.

## Known limitations

- Tokens and cost cover local sessions only. See [openai.md](models/openai.md#cloud-tasks) for OpenAI cloud tasks.
- Limits are account-wide; offline limits go stale; Claude and Command Code offline limits are estimates.
- Command Code support is built from the CLI's bundled code, not real session files. See [commandcode.md](models/commandcode.md).
- OpenCode has no limits, and only `opencode.db` is read. See [opencode.md](models/opencode.md).
- The TUI clips long lines instead of wrapping: the key hints below about 68 columns, and the empty-state message below about 85.
