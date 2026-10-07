# OpenAI (Codex)

The text output and TUI label this tool **OpenAI**, and its JSON/cache/redaction identifier is `openai`. Codex paths (`$CODEX_HOME`, `~/.codex`) and credentials are unchanged.

`used_percent` maps directly to `used_pct`: `used_percent: 4` shows as **4% used**, where the [Codex status UI](https://github.com/openai/codex/blob/main/codex-rs/tui/src/status/rate_limits.rs) shows `96% left`.

Implementation: `src/sources/openai.rs`, `src/live.rs` (`openai()`), `src/pricing.rs`.

## Logs

- Paths: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl` and `$CODEX_HOME/archived_sessions/rollout-*.jsonl` (default `~/.codex`). Codex moves old sessions to `archived_sessions`. A file name present in both is read once.
- Pre-filter: lines containing `"type":"turn_context"` or `"type":"token_count"`. The kind is re-checked after parsing.

### `turn_context`: sets the model for the following events in the file

```json
{"type": "turn_context", "payload": {"model": "gpt-6-astra", …}}
```

### `event_msg` / `token_count`: usage and limits

```json
{
  "type": "event_msg",
  "timestamp": "2026-09-17T04:40:18.348Z",
  "payload": {
    "type": "token_count",
    "info": {
      "last_token_usage":  {"input_tokens": 80520, "cached_input_tokens": 80384,
                            "cache_write_input_tokens": 0, "output_tokens": 102,
                            "reasoning_output_tokens": 0, "total_tokens": 80622},
      "total_token_usage": {"total_tokens": 744743, …}
    },
    "rate_limits": {
      "limit_id": "codex",
      "primary":   {"used_percent": 7.0,  "window_minutes": 300,   "resets_at": 1789637999},
      "secondary": {"used_percent": 89.0, "window_minutes": 10080, "resets_at": 1789806239},
      "plan_type": "plus"
    }
  }
}
```

## Mapping

| Event            | Field                                                                    |
| ---------------- | ------------------------------------------------------------------------ |
| `input`          | `input_tokens − cached_input_tokens − cache_write_input_tokens` (≥ 0)    |
| `output`         | `output_tokens` (already includes reasoning)                             |
| `cache_read`     | `cached_input_tokens`                                                    |
| `cache_write_5m` | `cache_write_input_tokens`                                               |
| `cache_write_1h` | 0                                                                        |

`total_tokens = input_tokens + output_tokens`, so `input_tokens` already includes the cached part.

## Quirks

- **Use `last_token_usage`, not `total_token_usage`.** The total is cumulative per session.
- **Repeated snapshots:** if `total_token_usage.total_tokens` equals the previous event's in the same file, the event is skipped.
- **Ignore `token_usage_record` lines.** They repeat the same usage.
- **Rate limits:** toknote takes the newest by line timestamp across all files, only for `limit_id` `codex` or missing. Windows are mapped by `window_minutes` (300 → 5h, 10080 → 7d), falling back to primary/secondary. Older Codex versions logged `resets_in_seconds` instead of `resets_at`; both are handled.
- **Model names:** `codex-auto-review` has no published price and shows `?`.

## Cloud tasks

Codex cloud tasks (ChatGPT-hosted threads) write **no rollout files**. Locally, Codex keeps only metadata in `~/.codex/sqlite/codex-dev.db` (`local_thread_catalog`, `host_id = chatgpt:…`): titles and timestamps, no token counts. The usage endpoint returns percentages only (`model_usage` just flags model availability). So local tokens and cost exclude cloud tasks, while limits (account-wide) include them.

## Live (`--live`)

- `GET https://chatgpt.com/backend-api/wham/usage` (undocumented; what Codex itself calls)
- Headers: `Authorization: Bearer <tokens.access_token>`, `ChatGPT-Account-Id: <tokens.account_id>`, `User-Agent: toknote/<version>`
- Credentials: `$CODEX_HOME/auth.json`. Requires `auth_mode: "chatgpt"`; API-key logins have no plan limits. toknote never refreshes tokens; on 401/403, run `codex`.
- Response fields used:

```json
{"rate_limit": {
  "primary_window":   {"used_percent": 16, "limit_window_seconds": 18000,  "reset_at": 1791236398},
  "secondary_window": {"used_percent": 43, "limit_window_seconds": 604800, "reset_at": 1791585429}
}}
```

Windows are mapped by `limit_window_seconds` (18000 → 5h, 604800 → 7d). Other fields (`code_review_rate_limit`, `additional_rate_limits`, `credits`, …) are ignored.

## Pricing

USD per 1M tokens, as of 2026-10-05, from [OpenAI API pricing](https://developers.openai.com/api/docs/pricing), Standard tier, short context. OpenAI has one cache-write price, used for both write columns. The source of truth is `src/pricing.rs`.

| Model        | Input | Cached input | Cache write | Output |
| ------------ | ----: | -----------: | ----------: | -----: |
| gpt-6-astra  | 10    | 1            | 12.50       | 50     |
| gpt-6.1-sol  | 2     | 0.10         | 2.50        | 10     |
| gpt-6-sol    | 2     | 0.20         | 2.50        | 10     |
| gpt-6-luna   | 0.10  | 0.01         | 0.125       | 0.50   |
| gpt-5.6-sol  | 4     | 0.40         | 5           | 20     |

- `gpt-6-sol` is no longer on the pricing page. Its price comes from the launch post ($2 in / $10 out, cached 90% off).
- Long-context prices (about 2× input) aren't applied, so very long Codex turns are underestimated.
