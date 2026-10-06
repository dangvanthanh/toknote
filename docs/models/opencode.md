# OpenCode

[OpenCode](https://opencode.ai/) is an open-source coding agent that works with any provider (Zen, Go, Fireworks, z.ai, your own keys, …).

Implementation: `src/sources/opencode.rs` (`rusqlite` with its bundled SQLite).

## Storage

- Database: `$XDG_DATA_HOME/opencode/opencode.db` (default `~/.local/share/opencode/opencode.db`, also on macOS). It's SQLite in WAL mode, managed by Drizzle.
- Opened **read-only** (`SQLITE_OPEN_READ_ONLY`), so it's safe while OpenCode is running.
- The legacy JSON store (`~/.local/share/opencode/storage/message/<session>/<msg>.json`) is **not read**. OpenCode migrates it into the database. Verified locally: all 737 legacy assistant messages are present in `opencode.db`.

### Tables

| Table     | Used | Contents                                                        |
| --------- | ---- | --------------------------------------------------------------- |
| `message` | yes  | One row per message; `data` is JSON (role, model, tokens, cost) |
| `part`    | no   | Message text, tool calls and output. Never read                 |
| `session` | no   | Per-session totals (`tokens_*`, `cost`), title, directory       |

### `message.data` for assistant rows (fields toknote reads)

```json
{
  "role": "assistant",
  "providerID": "fireworks-ai",
  "modelID": "accounts/fireworks/models/kimi-k2p5",
  "cost": 0.0123,
  "tokens": {
    "total": 30582,
    "input": 603,
    "output": 411,
    "reasoning": 0,
    "cache": {"read": 29568, "write": 0}
  },
  "time": {"created": 1788547427892, "completed": 1788547435930}
}
```

Older rows have no `tokens.total`. Other keys (`agent`, `mode`, `finish`, `path`, `parentID`) are ignored.

### Query

Only usage fields leave SQLite, selected with `json_extract`:

```sql
SELECT time_created,
       json_extract(data, '$.modelID'),
       json_extract(data, '$.tokens.input'),
       json_extract(data, '$.tokens.output'),
       json_extract(data, '$.tokens.reasoning'),
       json_extract(data, '$.tokens.cache.read'),
       json_extract(data, '$.tokens.cache.write'),
       json_extract(data, '$.cost')
FROM message
WHERE json_extract(data, '$.role') = 'assistant';
```

`time_created` is epoch milliseconds.

## Mapping

| Event       | Field                                  |
| ---------------- | -------------------------------------- |
| `input`          | `tokens.input` (already excludes cache) |
| `output`         | `tokens.output + tokens.reasoning`     |
| `cache_read`     | `tokens.cache.read`                    |
| `cache_write_5m` | `tokens.cache.write`                   |
| `cache_write_1h` | 0                                      |
| `cost_usd`       | `cost`                                 |
| `model`          | last `/` segment of `modelID`          |

`tokens.total = input + output + reasoning + cache.read + cache.write`, so the counters don't overlap. Reasoning is reported separately from output and billed as output.

## Quirks

- **No dedup needed:** `message.id` is the primary key. Sub-agent sessions (`session.parent_id`) have their own messages.
- **Model names** are shortened: `accounts/fireworks/models/kimi-k2p5` → `kimi-k2p5`. The same model from two providers shares a row.
- **Unreadable database** (corrupt, locked or newer schema) is reported as one skipped entry for `opencode.db` with `--verbose`. Other tools still load.

## Cost

OpenCode records `cost` per message, computed from [models.dev](https://models.dev) prices, and toknote uses it as is (labeled `est.`). Free models (e.g. Zen `big-pickle`, `*-free`) and subscription providers (e.g. `zai-coding-plan`) record `0`, which shows as `$0.00`, not `?`. toknote has no OpenCode price table.

## Limits

None. OpenCode itself has no usage limits. It uses your provider's.

OpenCode Go (a $10/mo subscription) has dollar-value caps: 5h $12, weekly $30, monthly $60 ([docs](https://opencode.ai/docs/go/)). These are server-side, with no documented endpoint, and their weekly and monthly boundaries are server-defined, so toknote shows neither an estimate nor live data. `--live` skips OpenCode.

## Known limitations

- Only `opencode.db` is read. Data that exists only in the legacy JSON store (OpenCode before the SQLite migration, never upgraded) isn't counted.
- `$0.00` for free and subscription models reflects OpenCode's recorded cost, not an API-equivalent price.
- No usage limits, including OpenCode Go caps.
