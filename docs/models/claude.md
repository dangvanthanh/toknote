# Claude Code

Implementation: `src/sources/claude.rs`, `src/live.rs` (`claude()`), `src/pricing.rs`.

## Logs

- Paths: `~/.claude/projects/**/*.jsonl` and `~/.config/claude/projects/**/*.jsonl`.
- Pre-filter: lines containing both `"type":"assistant"` and `"usage"`.
- Line shape (fields toknote reads):

```json
{
  "type": "assistant",
  "timestamp": "2026-10-05T20:59:19.277Z",
  "requestId": "req_…",
  "message": {
    "id": "msg_…",
    "model": "claude-opus-5-5",
    "usage": {
      "input_tokens": 2,
      "output_tokens": 2289,
      "cache_read_input_tokens": 0,
      "cache_creation_input_tokens": 19862,
      "cache_creation": {
        "ephemeral_5m_input_tokens": 0,
        "ephemeral_1h_input_tokens": 19862
      }
    }
  }
}
```

## Mapping

| Event       | Field                                             |
| ---------------- | ------------------------------------------------- |
| `input`          | `input_tokens`                                    |
| `output`         | `output_tokens`                                   |
| `cache_read`     | `cache_read_input_tokens`                         |
| `cache_write_5m` | `cache_creation.ephemeral_5m_input_tokens`        |
| `cache_write_1h` | `cache_creation.ephemeral_1h_input_tokens`        |

If `cache_creation` is missing, `cache_creation_input_tokens` counts as 5m writes.

## Quirks

- **Duplicates:** one API response is logged once per content block, so the same `message.id + requestId` repeats (over 1,000 pairs in a typical log set). Dedup keeps the first, across all files. Entries missing either id aren't deduped.
- **Model suffixes:** `claude-opus-5-5[1m]` (1M context) is priced as its base model. Since Claude 4.6, 1M context is billed at standard rates.
- **`<synthetic>`** entries are skipped.
- **No limits** are written to the logs.

## Limits

### Offline estimate (`est`)

The current 5h block comes from local timestamps: it starts at the first event after the previous block ended and lasts 5h. toknote shows the reset time and the block's tokens, with no percent. Usage on other machines isn't seen.

### Live (`--live`)

- `GET https://api.anthropic.com/api/oauth/usage` (undocumented; the data behind Claude Code `/usage`)
- Headers: `Authorization: Bearer <accessToken>`, `anthropic-beta: oauth-2025-04-20`
- Credentials: macOS Keychain service `Claude Code-credentials` first, then `~/.claude/.credentials.json`. Shape: `{"claudeAiOauth": {"accessToken", "expiresAt" (ms), …}}`. The first unexpired one wins. toknote never refreshes tokens; if expired, run `claude`.
- Response fields used: `five_hour` / `seven_day` → `utilization` (0–100), `resets_at` (RFC 3339). The response also has many other, mostly null, windows (`seven_day_opus`, `extra_usage`, `limits`, …); toknote ignores them.
- The endpoint rate-limits (HTTP 429), especially since Claude Code polls it too. Hence the 60s cache.

## Pricing

USD per 1M tokens, as of 2026-10-05, from [platform.claude.com pricing](https://platform.claude.com/docs/en/about-claude/pricing). The source of truth is `src/pricing.rs`.

| Model               | Input | Output | 5m write | 1h write | Cache read |
| ------------------- | ----: | -----: | -------: | -------: | ---------: |
| claude-fable-5-1    | 10    | 50     | 12.50    | 20       | 0.25       |
| claude-opus-5-5     | 4     | 20     | 5        | 8        | 0.20       |
| claude-sonnet-5-5   | 2     | 10     | 2.50     | 4        | 0.20       |
| claude-haiku-4-5    | 1     | 5      | 1.25     | 2        | 0.10       |
| claude-opus-5       | 5     | 25     | 6.25     | 10       | 0.50       |
| claude-sonnet-5     | 2     | 10     | 2.50     | 4        | 0.20       |
| claude-opus-4-5…4-8 | 5     | 25     | 6.25     | 10       | 0.50       |
| claude-sonnet-4-5/4-6 | 3   | 15     | 3.75     | 6        | 0.30       |
