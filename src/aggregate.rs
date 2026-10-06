use std::collections::HashMap;

use crate::cli::Period;
use crate::model::{Event, Limit, Origin, Scan, Tool, Window};
use crate::pricing;
use crate::time;

#[derive(Clone, Debug)]
pub struct Row {
    pub tool: Tool,
    pub model: String,
    pub tokens: u64,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Totals {
    pub tokens: u64,
    pub cost_usd: f64,
    pub unpriced_tokens: u64,
}

#[derive(Debug)]
pub struct Summary {
    pub period: Period,
    pub start: i64,
    pub end: i64,
    pub rows: Vec<Row>,
}

impl Summary {
    pub fn totals(&self, tool: Option<Tool>) -> Totals {
        let mut t = Totals::default();
        for r in self.rows.iter().filter(|r| tool.is_none_or(|x| x == r.tool)) {
            t.tokens = t.tokens.saturating_add(r.tokens);
            match r.cost_usd {
                Some(x) => t.cost_usd += x,
                None => t.unpriced_tokens = t.unpriced_tokens.saturating_add(r.tokens),
            }
        }
        t
    }
}

pub fn start(p: Period, now: i64) -> i64 {
    match p {
        Period::Today => time::midnight(now, 0),
        Period::Week => time::midnight(now, -time::weekday_from_monday(now)),
        Period::Month => time::midnight(now, 1 - time::day_of_month(now)),
        Period::Days7 => now - 7 * time::DAY,
        Period::Days30 => now - 30 * time::DAY,
    }
}

/// Groups events in the period by `(tool, model)`. A row with any unpriced event is unpriced.
pub fn summarize(events: &[Event], p: Period, now: i64) -> Summary {
    let begin = start(p, now);
    let mut rows: Vec<Row> = Vec::new();
    let mut index: HashMap<(Tool, &str), usize> = HashMap::new();
    for e in events {
        if e.ts < begin || e.ts > now || e.tokens() == 0 {
            continue;
        }
        let i = *index.entry((e.tool, &e.model)).or_insert_with(|| {
            rows.push(Row { tool: e.tool, model: e.model.clone(), tokens: 0, cost_usd: Some(0.0) });
            rows.len() - 1
        });
        let row = &mut rows[i];
        row.tokens = row.tokens.saturating_add(e.tokens());
        if let Some(old) = row.cost_usd {
            row.cost_usd = pricing::cost(e).map(|c| old + c);
        }
    }
    rows.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
    Summary { period: p, start: begin, end: now, rows }
}

/// Estimated windows from local timestamps: a window opens at the first event at or after the
/// previous window's end. Only a window that hasn't ended yet is reported.
pub fn estimates(s: &mut Scan, now: i64) {
    let mut sorted: Vec<(i64, Tool, u64)> = s.events.iter().map(|e| (e.ts, e.tool, e.tokens())).collect();
    sorted.sort_by_key(|e| e.0);
    for tool in [Tool::Claude, Tool::Commandcode] {
        for window in [Window::FiveHour, Window::Week] {
            if tool == Tool::Claude && window == Window::Week {
                continue;
            }
            let mut end: Option<i64> = None;
            let mut tokens: u64 = 0;
            for &(ts, t, n) in &sorted {
                if t != tool {
                    continue;
                }
                if end.is_none_or(|x| ts >= x) {
                    let Some(next) = ts.checked_add(window.length()) else { continue };
                    end = Some(next);
                    tokens = n;
                } else {
                    tokens = tokens.saturating_add(n);
                }
            }
            if let Some(t) = end
                && t > now
            {
                s.limits.push(Limit { resets_at: Some(t), block_tokens: Some(tokens), ..Limit::new(tool, window, Origin::Est) });
            }
        }
    }
}
