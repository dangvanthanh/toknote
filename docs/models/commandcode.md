# Command Code

[Command Code](https://commandcode.ai/) (`npm i -g command-code`, binary `cmd`) is a coding agent for open models.

Implementation: `src/sources/commandcode.rs`, `src/live.rs` (`commandcode()`), `src/aggregate.rs` (`estimates`).

> The formats below were read from the CLI's bundled code (`command-code` 1.74.2), not from real session files. The parser was checked against a hand-written session. Re-check against real logs when possible.

## Logs

- Path: `~/.commandcode/projects/<project-slug>/<session-id>.jsonl`, one append-only transcript per session.
- Only `<id>.jsonl` files are read. Sidecars in the same folder are skipped: `<id>.meta.json`, `<id>.share.json`, `<id>.checkpoints.jsonl` (snapshots could repeat messages) and `<id>.prompts.jsonl`.
- `cmd --no-session` sessions are never written, so they can't be counted.
- Pre-filter: lines containing both `"type":"message"` and `"usage"`. After parsing, toknote checks `type == "message"` and `message.role == "assistant"`.

The first line is a header; every later line is an entry in a tree (`parentId`):

```json
{"type": "session", "version": 3, "id": "…", "timestamp": "…", "cwd": "/path", "parentSession": "…?"}
```

Assistant replies (fields toknote reads):

```json
{
  "type": "message",
  "id": "e2",
  "parentId": "e1",
  "timestamp": "2026-10-05T20:59:19.277Z",
  "message": {"role": "assistant", "meta": {"messageId": "m2"}, "content": […]},
  "model": "glm-5.2",
  "usage": {
    "inputTokens": 10000,
    "outputTokens": 500,
    "cacheReadTokens": 8000,
    "cacheWriteTokens": 1000,
    "cacheWriteTokens1h": 400,
    "costUsd": 0.0123
  }
}
```

Other entry types (`model_change`, `effort_change`, compaction, custom) are ignored. `timestamp` is parsed as RFC 3339, or as unix seconds or milliseconds.

## Mapping

| Event       | Field                                                            |
| ---------------- | ---------------------------------------------------------------- |
| `input`          | `inputTokens − cacheReadTokens − cacheWriteTokens` (≥ 0)         |
| `output`         | `outputTokens`                                                   |
| `cache_read`     | `cacheReadTokens`                                                |
| `cache_write_5m` | `cacheWriteTokens − cacheWriteTokens1h`                          |
| `cache_write_1h` | `cacheWriteTokens1h` (optional, Anthropic models only)           |
| `cost_usd`       | `costUsd` (optional)                                             |

`inputTokens` follows AI SDK semantics: it's the total prompt, cached parts included. This is unverified against real logs; if it turns out to exclude cache, input tokens are undercounted.

## Quirks

- **Duplicates:** `/fork` copies the whole session into a new file, and `/clone` copies the active branch. toknote dedups by `message.meta.messageId` (falling back to the entry `id`) across all files.
- **Branches:** `/rewind` and `/tree` move a pointer but never delete entries. Abandoned branches still count, since their requests were made and billed.
- **Models:** about 50 open and closed models (GLM, DeepSeek, Kimi, Claude, …). Names are kept as logged.

## Cost

Command Code records its own estimate per reply in `usage.costUsd`, and toknote uses it as is. It's still labeled `est.`; on credit-based plans (Go, GOAT, Pro), credits are usage-value units, not dollars. Replies without `costUsd` show `?`. toknote has no Command Code price table.

## Limits

Command Code plans have two rolling windows measured in credits: 5h and weekly. Each window **opens on your first request and resets exactly 5h / 7d later**. Pay-as-you-go and Enterprise pools have no windows. Extra (pay-as-you-go) credits are never capped.

| Plan     | 5h cap | Weekly cap |
| -------- | -----: | ---------: |
| Go       | 3      | 6          |
| GOAT     | 14     | 35         |
| Pro      | 16     | 40         |
| Max 10×  | 45     | 90         |
| Max 20×  | 90     | 180        |
| Team Pro | 12     | 24         |

Source: [Usage Limits](https://commandcode.ai/docs/resources/usage-limits) (2026-10-05).

### Offline estimate (`est`)

toknote estimates both windows from local timestamps, using the same rule as Claude's 5h block: a window opens at the first reply after the previous one ended. It shows reset times and tokens, with no percent, because caps are in credits and depend on the plan. Usage on other machines isn't seen.

### Live (`--live`)

- `GET https://api.commandcode.ai/alpha/billing/credits` (undocumented; what `cmd /usage` calls)
- Header: `Authorization: Bearer <apiKey>`
- Credentials: `$COMMAND_CODE_API_KEY` first, then `~/.commandcode/auth.json` (`{"apiKey", "userName", …}`). If neither exists, Command Code is skipped silently.
- Response fields used:

```json
{"windowLimits": {
  "limited": true,
  "fiveHour": {"used": 4.2, "cap": 14, "resetAt": 1791236398000},
  "weekly":   {"used": 12.0, "cap": 35, "resetAt": 1791585429000}
}}
```

- Percent is `used / cap × 100`, capped at 100. `resetAt` is read as epoch milliseconds or seconds, or as RFC 3339. The CLI compares it to `Date.now()`, so it's most likely milliseconds.
- `limited: false` or a missing `windowLimits` gives the note `commandcode: no plan limits`.
- On 401/403, run `cmd login`.

## Known limitations

- Not yet verified against real session files or a real API response. See the note at the top.
- Cost is Command Code's own estimate. Credit-plan users are billed in credits, not these dollars.
- Offline limits are estimates with no percent. Use `--live` for real credit usage.
