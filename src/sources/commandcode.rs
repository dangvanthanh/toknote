use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::files::{self as f, FileLines, Parsed, ReadFailed};
use crate::ctx::Ctx;
use crate::model::{Event, Scan, Tool};
use crate::time;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    timestamp: Value,
    message: Option<Message>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Message {
    role: Option<String>,
    meta: Option<Meta>,
}

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "messageId")]
    message_id: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(rename = "inputTokens")]
    input: Option<u64>,
    #[serde(rename = "outputTokens")]
    output: Option<u64>,
    #[serde(rename = "cacheReadTokens")]
    cache_read: Option<u64>,
    #[serde(rename = "cacheWriteTokens")]
    cache_write: Option<u64>,
    #[serde(rename = "cacheWriteTokens1h")]
    cache_write_1h: Option<u64>,
    #[serde(rename = "costUsd")]
    cost_usd: Option<f64>,
}

/// Session transcripts are `<id>.jsonl`; sidecars like `<id>.checkpoints.jsonl` repeat messages.
pub fn files(ctx: &Ctx, s: &mut Scan) -> Vec<PathBuf> {
    let root = ctx.home.join(".commandcode/projects");
    if root.is_dir() {
        s.found[Tool::Commandcode.index()] = true;
    }
    let mut all = f::jsonl(&root, "");
    all.retain(|p| p.file_stem().is_some_and(|stem| !stem.as_encoded_bytes().contains(&b'.')));
    all
}

/// Runs on a worker thread. An unreadable file contributes nothing.
pub fn parse(path: &Path) -> Parsed {
    let mut out = Parsed::new(path);
    match parse_file(path, &mut out) {
        Ok(()) => out,
        Err(ReadFailed) => Parsed::new(path),
    }
}

fn parse_file(path: &Path, out: &mut Parsed) -> Result<(), ReadFailed> {
    let Some(mut lines) = FileLines::open(path, &[b"\"type\":\"message\""]) else { return Ok(()) };
    while let Some(line) = lines.next()? {
        if !f::contains(line, b"\"usage\"") {
            continue;
        }
        let Ok(p) = f::parse::<Line>(line) else {
            out.skipped += 1;
            continue;
        };
        let Some(msg) = p.message else { continue };
        if p.kind != "message" || msg.role.as_deref() != Some("assistant") {
            continue;
        }
        let Some(u) = p.usage else { continue };
        let Some(ts) = time::parse_value(&p.timestamp) else {
            out.skipped += 1;
            continue;
        };
        // `/fork` and `/clone` copy entries into new files.
        out.keys.push(msg.meta.and_then(|m| m.message_id).or(p.id));
        let read = u.cache_read.unwrap_or(0);
        let write = u.cache_write.unwrap_or(0);
        let w1 = u.cache_write_1h.unwrap_or(0).min(write);
        out.events.push(Event {
            input: u.input.unwrap_or(0).saturating_sub(read.saturating_add(write)),
            output: u.output.unwrap_or(0),
            cache_read: read,
            cache_write_5m: write - w1,
            cache_write_1h: w1,
            cost_usd: u.cost_usd,
            ..Event::new(Tool::Commandcode, ts, p.model.unwrap_or_else(|| "unknown".into()))
        });
    }
    Ok(())
}
