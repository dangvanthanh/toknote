use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::files::{self as f, FileLines, ReadFailed};
use crate::ctx::Ctx;
use crate::model::{Event, Limit, Origin, Scan, Tool, Window};
use crate::time;

#[derive(Deserialize)]
struct Tokens {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    cache_write_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct RateWindow {
    used_percent: Option<f32>,
    window_minutes: Option<u64>,
    resets_at: Option<i64>,
    resets_in_seconds: Option<i64>,
}

#[derive(Deserialize)]
struct Rates {
    limit_id: Option<String>,
    primary: Option<RateWindow>,
    secondary: Option<RateWindow>,
}

#[derive(Deserialize)]
struct Info {
    last_token_usage: Option<Tokens>,
    total_token_usage: Option<Tokens>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    model: Option<String>,
    info: Option<Info>,
    rate_limits: Option<Rates>,
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    timestamp: String,
    payload: Option<Payload>,
}

/// A rollout's events plus its newest `rate_limits` (last line wins on equal timestamps).
#[derive(Debug, Default)]
pub struct Parsed {
    pub base: f::Parsed,
    latest: Option<i64>,
    rates: [Option<RateWindow>; 2],
}

/// Active sessions first; a file name present in both roots is read once.
pub fn files(ctx: &Ctx, s: &mut Scan) -> Vec<PathBuf> {
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    for root in [ctx.openai_home.join("sessions"), ctx.openai_home.join("archived_sessions")] {
        if root.is_dir() {
            s.found[Tool::Openai.index()] = true;
        }
        for path in f::jsonl(&root, "rollout-") {
            if seen.insert(path.file_name().map(ToOwned::to_owned)) {
                all.push(path);
            }
        }
    }
    all
}

/// Runs on a worker thread. An unreadable file contributes nothing.
pub fn parse(path: &Path) -> Parsed {
    let mut out = Parsed { base: f::Parsed::new(path), ..Parsed::default() };
    match parse_file(path, &mut out) {
        Ok(()) => out,
        Err(ReadFailed) => Parsed { base: f::Parsed::new(path), ..Parsed::default() },
    }
}

fn parse_file(path: &Path, out: &mut Parsed) -> Result<(), ReadFailed> {
    let mut model = String::from("unknown");
    let mut previous: Option<u64> = None;
    let Some(mut lines) = FileLines::open(path, &[b"\"type\":\"turn_context\"", b"\"type\":\"token_count\""]) else { return Ok(()) };
    while let Some(line) = lines.next()? {
        let Ok(p) = f::parse::<Line>(line) else {
            out.base.skipped += 1;
            continue;
        };
        let Some(ts) = time::parse(&p.timestamp) else {
            out.base.skipped += 1;
            continue;
        };
        let Some(payload) = p.payload else { continue };
        if p.kind == "turn_context" {
            if let Some(x) = payload.model {
                model = x;
            }
            continue;
        }
        if p.kind != "event_msg" || payload.kind.as_deref() != Some("token_count") {
            continue;
        }
        if let Some(rl) = payload.rate_limits
            && rl.limit_id.as_deref().unwrap_or("codex") == "codex"
            && out.latest.is_none_or(|l| ts >= l)
        {
            out.latest = Some(ts);
            out.rates = [rl.primary, rl.secondary];
        }
        let Some(info) = payload.info else { continue };
        let total = info.total_token_usage.and_then(|t| t.total_tokens);
        if total.is_some() && total == previous {
            continue;
        }
        previous = total;
        let Some(u) = info.last_token_usage else { continue };
        let cache = u.cached_input_tokens.unwrap_or(0);
        let write = u.cache_write_input_tokens.unwrap_or(0);
        out.base.events.push(Event {
            input: u.input_tokens.unwrap_or(0).saturating_sub(cache.saturating_add(write)),
            output: u.output_tokens.unwrap_or(0),
            cache_read: cache,
            cache_write_5m: write,
            ..Event::new(Tool::Openai, ts, model.clone())
        });
    }
    Ok(())
}

/// Merges in file order. Taking each file's newest limits with `>=` picks the same line as one
/// sequential scan: the last occurrence of the newest timestamp.
pub fn merge(s: &mut Scan, results: Vec<Parsed>, now: i64) {
    let mut seen = HashSet::new();
    let mut latest: Option<i64> = None;
    let mut rates = [None; 2];
    for r in results {
        f::merge(s, &mut seen, r.base);
        if let Some(ts) = r.latest
            && latest.is_none_or(|l| ts >= l)
        {
            latest = Some(ts);
            rates = r.rates;
        }
    }
    let Some(ts) = latest else { return };
    for (i, rw) in rates.into_iter().enumerate() {
        let Some(w) = rw else { continue };
        let Some(used) = w.used_percent else { continue };
        let window = match w.window_minutes {
            Some(300) => Window::FiveHour,
            Some(10080) => Window::Week,
            Some(_) => continue,
            None if i == 0 => Window::FiveHour,
            None => Window::Week,
        };
        let reset = if let Some(v) = w.resets_at {
            v.checked_mul(time::SECOND)
        } else if let Some(v) = w.resets_in_seconds {
            let Some(d) = v.checked_mul(time::SECOND) else { continue };
            ts.checked_add(d)
        } else {
            None
        };
        let l = Limit { used_pct: Some(used), resets_at: reset, ..Limit::new(Tool::Openai, window, Origin::Log) };
        s.limits.push(l.expire(now));
    }
}
