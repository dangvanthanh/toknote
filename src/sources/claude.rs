use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::files::{self as f, FileLines, Parsed, ReadFailed};
use crate::ctx::Ctx;
use crate::model::{Event, Scan, Tool};
use crate::time;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    timestamp: String,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

pub fn files(ctx: &Ctx, s: &mut Scan) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for root in [ctx.home.join(".claude/projects"), ctx.home.join(".config/claude/projects")] {
        if root.is_dir() {
            s.found[Tool::Claude.index()] = true;
        }
        all.extend(f::jsonl(&root, ""));
    }
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
    let Some(mut lines) = FileLines::open(path, &[b"\"type\":\"assistant\""]) else { return Ok(()) };
    while let Some(line) = lines.next()? {
        if !f::contains(line, b"\"usage\"") {
            continue;
        }
        let Ok(p) = f::parse::<Line>(line) else {
            out.skipped += 1;
            continue;
        };
        let Some(ts) = time::parse(&p.timestamp) else {
            out.skipped += 1;
            continue;
        };
        if p.kind != "assistant" {
            continue;
        }
        let Some(msg) = p.message else { continue };
        let Some(u) = msg.usage else { continue };
        let model = msg.model.unwrap_or_else(|| "unknown".into());
        if model == "<synthetic>" {
            continue;
        }
        // One API response is logged once per content block: dedup `message.id + requestId`.
        let key = msg.id.zip(p.request_id).map(|(id, rid)| format!("{}:{id}{rid}", id.len()));
        out.keys.push(key);
        let (w5, w1) = match u.cache_creation {
            Some(cc) => (cc.ephemeral_5m_input_tokens.unwrap_or(0), cc.ephemeral_1h_input_tokens.unwrap_or(0)),
            None => (u.cache_creation_input_tokens.unwrap_or(0), 0),
        };
        out.events.push(Event {
            input: u.input_tokens.unwrap_or(0),
            output: u.output_tokens.unwrap_or(0),
            cache_read: u.cache_read_input_tokens.unwrap_or(0),
            cache_write_5m: w5,
            cache_write_1h: w1,
            ..Event::new(Tool::Claude, ts, model)
        });
    }
    Ok(())
}
