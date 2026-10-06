use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use serde::Serialize;

use crate::aggregate::{self, Summary, Totals};
use crate::ctx::Ctx;
use crate::json::{self, LimitJson, Num};
use crate::model::{self, Limit, Origin, Scan, Tool};
use crate::sources::{claude, commandcode, files, openai, opencode};
use crate::time;

pub const EMPTY: &str = "No usage found in ~/.claude, ~/.codex, ~/.commandcode or ~/.local/share/opencode";

#[derive(Clone, Copy)]
enum Job {
    Opencode,
    Claude(usize),
    Openai(usize),
    Commandcode(usize),
}

enum Done {
    Opencode(Scan),
    Claude(usize, files::Parsed),
    Openai(usize, openai::Parsed),
    Commandcode(usize, files::Parsed),
}

/// Parses every log file in parallel, then merges in discovery order, so the result is identical
/// to a sequential scan. Workers pull the next job until none remain, largest file first.
pub fn load(ctx: &Ctx, now: i64) -> Scan {
    let mut s = Scan::default();
    let claude_files = claude::files(ctx, &mut s);
    let openai_files = openai::files(ctx, &mut s);
    let cc_files = commandcode::files(ctx, &mut s);

    let mut jobs = vec![(u64::MAX, Job::Opencode)];
    jobs.extend(claude_files.iter().enumerate().map(|(k, p)| (files::size(p), Job::Claude(k))));
    jobs.extend(openai_files.iter().enumerate().map(|(k, p)| (files::size(p), Job::Openai(k))));
    jobs.extend(cc_files.iter().enumerate().map(|(k, p)| (files::size(p), Job::Commandcode(k))));
    jobs.sort_by_key(|j| std::cmp::Reverse(j.0));

    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, |n| n.get()).min(jobs.len());
    let done: Vec<Done> = thread::scope(|scope| {
        let worker = || {
            let mut out = Vec::new();
            while let Some(&(_, job)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                out.push(match job {
                    // The single SQLite job opens, queries and closes the database on one thread.
                    Job::Opencode => Done::Opencode(opencode::scan(ctx)),
                    Job::Claude(k) => Done::Claude(k, claude::parse(&claude_files[k])),
                    Job::Openai(k) => Done::Openai(k, openai::parse(&openai_files[k])),
                    Job::Commandcode(k) => Done::Commandcode(k, commandcode::parse(&cc_files[k])),
                });
            }
            out
        };
        let handles: Vec<_> = (0..workers).map(|_| scope.spawn(worker)).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    });

    let mut claude_out: Vec<files::Parsed> = claude_files.iter().map(|p| files::Parsed::new(p)).collect();
    let mut openai_out: Vec<openai::Parsed> = openai_files.iter().map(|_| openai::Parsed::default()).collect();
    let mut cc_out: Vec<files::Parsed> = cc_files.iter().map(|p| files::Parsed::new(p)).collect();
    let mut oc = Scan::default();
    for d in done {
        match d {
            Done::Opencode(x) => oc = x,
            Done::Claude(k, r) => claude_out[k] = r,
            Done::Openai(k, r) => openai_out[k] = r,
            Done::Commandcode(k, r) => cc_out[k] = r,
        }
    }

    let mut seen = HashSet::new();
    for r in claude_out {
        files::merge(&mut s, &mut seen, r);
    }
    openai::merge(&mut s, openai_out, now);
    seen.clear();
    for r in cc_out {
        files::merge(&mut s, &mut seen, r);
    }
    s.events.append(&mut oc.events);
    s.skipped.append(&mut oc.skipped);
    s.found[Tool::Opencode.index()] = oc.found[Tool::Opencode.index()];
    aggregate::estimates(&mut s, now);
    s
}

pub fn cost_text(t: Totals) -> String {
    if t.tokens > 0 && t.tokens == t.unpriced_tokens {
        "?".into()
    } else if t.unpriced_tokens > 0 {
        format!("{}+?", model::fmt_cost(t.cost_usd))
    } else {
        model::fmt_cost(t.cost_usd)
    }
}

pub fn origin(l: &Limit, now: i64) -> String {
    if l.origin == Origin::Live
        && let Some(t) = l.fetched_at
    {
        let age = (now - t) / (60 * time::SECOND);
        if age >= 1 {
            return format!("live {age}m ago");
        }
    }
    l.origin.id().into()
}

pub fn text(s: &Summary, scan: &Scan, now: i64) -> String {
    let mut w = String::new();
    let all = s.totals(None);
    let _ = writeln!(w, "{}  {} est · {} tok", s.period.label(), cost_text(all), model::fmt_tokens(all.tokens));
    let width = Tool::ALL.iter().filter(|&&t| scan.visible(t)).map(|t| t.name().len()).max().unwrap_or(0);
    for t in Tool::ALL.into_iter().filter(|&t| scan.visible(t)) {
        let totals = s.totals(Some(t));
        let _ = write!(w, "{:width$} {} {}", t.name(), cost_text(totals), model::fmt_tokens(totals.tokens));
        let mut count = 0;
        for l in scan.limits.iter().filter(|l| l.tool == t) {
            w.push_str(if count == 0 { "   " } else { " · " });
            count += 1;
            w.push_str(l.window.label());
            if let Some(pct) = l.used_pct {
                let _ = write!(w, " {}% used", model::fixed(pct, 0));
            }
            if let Some(reset) = l.resets_at {
                let _ = write!(w, " ↻{}", time::reset(reset, now));
            }
            if l.origin != Origin::Log {
                let _ = write!(w, " {}", origin(l, now));
            }
        }
        if count == 0 && t.estimates() {
            w.push_str("   5h idle");
        }
        w.push('\n');
    }
    w
}

#[derive(Serialize)]
struct TotalsJson {
    cost_usd: Num,
    tokens: u64,
    unpriced_tokens: u64,
}

#[derive(Serialize)]
struct RowJson<'a> {
    cost_usd: Num,
    model: &'a str,
    tokens: u64,
}

#[derive(Serialize)]
struct ToolJson<'a> {
    cost_usd: Num,
    models: Vec<RowJson<'a>>,
    tokens: u64,
    tool: &'static str,
    unpriced_tokens: u64,
}

#[derive(Serialize)]
struct Root<'a> {
    end: Option<String>,
    limits: Vec<LimitJson>,
    period: &'static str,
    skipped_lines: usize,
    start: Option<String>,
    tools: Vec<ToolJson<'a>>,
    total: TotalsJson,
}

/// Every tool is listed, visible or not.
pub fn json(s: &Summary, scan: &Scan) -> String {
    let tools = Tool::ALL
        .into_iter()
        .map(|t| {
            let totals = s.totals(Some(t));
            ToolJson {
                cost_usd: Num(Some(totals.cost_usd)),
                models: s.rows.iter().filter(|r| r.tool == t).map(|r| RowJson { cost_usd: Num(r.cost_usd), model: &r.model, tokens: r.tokens }).collect(),
                tokens: totals.tokens,
                tool: t.id(),
                unpriced_tokens: totals.unpriced_tokens,
            }
        })
        .collect();
    let total = s.totals(None);
    let root = Root {
        end: json::timestamp(Some(s.end)),
        limits: scan.limits.iter().map(LimitJson::from).collect(),
        period: s.period.id(),
        skipped_lines: scan.skipped_total(),
        start: json::timestamp(Some(s.start)),
        tools,
        total: TotalsJson { cost_usd: Num(Some(total.cost_usd)), tokens: total.tokens, unpriced_tokens: total.unpriced_tokens },
    };
    let mut out = serde_json::to_string_pretty(&root).unwrap_or_default();
    out.push('\n');
    out
}

/// Write errors (e.g. a closed pipe) are ignored; Rust already ignores `SIGPIPE`.
pub fn write_out(s: &str) {
    let _ = std::io::stdout().lock().write_all(s.as_bytes());
}

pub fn write_err(s: &str) {
    let _ = std::io::stderr().lock().write_all(s.as_bytes());
}

pub fn skipped(ctx: &Ctx, s: &Scan, redact: bool) {
    for x in &s.skipped {
        let shown = if !redact {
            x.path.display().to_string()
        } else if x.path.starts_with(&ctx.openai_home) {
            "openai".into()
        } else if x.path.starts_with(ctx.home.join(".commandcode")) {
            "commandcode".into()
        } else if x.path.starts_with(ctx.data_home.join("opencode")) {
            "opencode".into()
        } else {
            "claude".into()
        };
        write_err(&format!("skipped {} lines in {shown}\n", x.count));
    }
}
