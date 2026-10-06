use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::app::App;
use super::term::safe;
use crate::aggregate;
use crate::cli::Period;
use crate::features;
use crate::model::{self, Limit, Tool, Window};
use crate::output;
use crate::time;

const NORMAL: Style = Style::new();
const DIM: Style = Style::new().add_modifier(Modifier::DIM);
const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);
/// The only colors: one soft pastel palette, in truecolor.
const BLUE: Style = Style::new().fg(Color::Rgb(137, 180, 250));
const GREEN: Style = Style::new().fg(Color::Rgb(166, 227, 161));
const YELLOW: Style = Style::new().fg(Color::Rgb(249, 226, 175));
const RED: Style = Style::new().fg(Color::Rgb(243, 139, 168));
/// Selection, active tab and the period total.
const ACCENT: Style = BLUE.add_modifier(Modifier::BOLD);
const KEYS: &str = "↑↓ select · ⏎ models · d/w/m period · r refresh · q quit";
const LOGO_WIDTH: usize = 11;
/// "[Today] " + " Week  " + " Month "
const TABS_WIDTH: usize = 23;

/// Styled lines, built in writing order. Ratatui clips them to the terminal width and height.
#[derive(Default)]
struct Screen {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
}

impl Screen {
    fn put(&mut self, s: impl Into<String>, style: Style) {
        self.spans.push(Span::styled(s.into(), style));
    }

    fn end(&mut self) {
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
    }

    fn padded(&mut self, s: &str, n: usize, right: bool, style: Style) {
        let pad = " ".repeat(n.saturating_sub(s.chars().count()));
        self.put(if right { format!("{pad}{s}") } else { format!("{s}{pad}") }, style);
    }

    /// "TN" in half blocks, two rows tall: T blue, N yellow.
    fn logo(&mut self, row: usize) {
        let (t, n) = [("▀█▀ ", "█▄ █"), (" █  ", "█ ▀█")][row];
        self.put(" ", NORMAL);
        self.put(t, BLUE);
        self.put(n, YELLOW);
        self.put("  ", NORMAL);
    }
}

fn relative(t: i64, now: i64) -> String {
    let minutes = ((t - now) / (60 * time::SECOND)).max(0);
    let clock = time::format(t, "%H:%M");
    match minutes {
        0 => format!("now · {clock}"),
        1..60 => format!("in {minutes}m · {clock}"),
        60..1440 if minutes % 60 == 0 => format!("in {}h · {clock}", minutes / 60),
        60..1440 => format!("in {}h {}m · {clock}", minutes / 60, minutes % 60),
        _ => time::reset(t, now),
    }
}

fn range_text(start: i64, end: i64) -> String {
    let (s, e) = (time::local(start), time::local(end));
    let first = time::format(start, "%b %-d");
    if s.date_naive() == e.date_naive() {
        first
    } else if chrono::Datelike::month(&s) == chrono::Datelike::month(&e) {
        format!("{first}–{}", chrono::Datelike::day(&e))
    } else {
        format!("{first}–{}", time::format(end, "%b %-d"))
    }
}

fn age_text(age: i64) -> String {
    if age < 1 { "just now".into() } else { format!("{age}m ago") }
}

fn status(app: &App) -> String {
    let mut has_live = false;
    let mut oldest = 0;
    let mut local = Vec::new();
    // OpenCode has no limits, so it isn't a local-limit fallback.
    for t in Tool::ALL.into_iter().filter(|&t| t != Tool::Opencode && app.scan.visible(t)) {
        let mut live = false;
        for l in app.scan.limits.iter().filter(|l| l.tool == t && l.origin == model::Origin::Live) {
            live = true;
            oldest = oldest.max(l.fetched_at.map_or(0, |at| (app.now - at) / (60 * time::SECOND)));
        }
        if live { has_live = true } else { local.push(t.name()) }
    }
    if !has_live {
        return if features::LIVE { "limits: local logs · press l for live" } else { "limits: local logs" }.into();
    }
    if local.is_empty() {
        return format!("limits: live · {} · l off", age_text(oldest));
    }
    format!("limits: live · {} · local: {} · l off", age_text(oldest), local.join(", "))
}

fn limit_line(w: &mut Screen, l: &Limit, now: i64) {
    w.put(format!("     {}  ", l.window.label()), NORMAL);
    if let Some(pct) = l.used_pct {
        // Usage zones: safe (0–49%), warning (50–79%), critical (80%+).
        let color = if pct >= 80.0 {
            RED
        } else if pct >= 50.0 {
            YELLOW
        } else {
            GREEN
        };
        let filled = (pct / 100.0 * 12.0).round().clamp(0.0, 12.0) as usize;
        w.put("█".repeat(filled), color);
        w.put("░".repeat(12 - filled), color.add_modifier(Modifier::DIM));
        w.put(format!(" {:>4}% used", model::fixed(pct, 0)), color);
        if pct >= 100.0 {
            w.put("  limit reached", color.add_modifier(Modifier::BOLD));
        }
        if let Some(r) = l.resets_at {
            w.put(format!("  resets {}", relative(r, now)), NORMAL);
        }
    } else {
        let mut text = match l.resets_at {
            Some(r) => format!("block ends {}", relative(r, now)),
            None => "no active block".into(),
        };
        if let Some(n) = l.block_tokens {
            text.push_str(&format!(" · {} tok", model::fmt_tokens(n)));
        }
        w.put(text, NORMAL);
        w.put(" (estimate)", DIM);
    }
    w.end();
}

pub fn draw(app: &App, cols: usize) -> Vec<Line<'static>> {
    let mut w = Screen::default();
    let version = format!("v{}", features::VERSION);
    w.logo(0);
    w.put(version.clone(), DIM);
    w.put(" ".repeat(2.max(cols.saturating_sub(LOGO_WIDTH + version.len() + TABS_WIDTH))), NORMAL);
    for p in [Period::Today, Period::Week, Period::Month] {
        if p == app.period {
            w.put(format!("[{}]", p.label()), ACCENT);
        } else {
            w.put(format!(" {} ", p.label()), DIM);
        }
        w.put(" ", NORMAL);
    }
    w.end();
    w.logo(1);
    w.put(KEYS, DIM);
    w.end();
    w.end();
    if !app.scan.any_found() {
        w.put(format!("  {}", output::EMPTY), NORMAL);
        w.end();
        w.end();
    } else {
        let s = aggregate::summarize(&app.scan.events, app.period, app.summary_now);
        let totals = s.totals(None);
        w.put("  ", NORMAL);
        w.put(format!("{} est", output::cost_text(totals)), ACCENT);
        w.put(format!(" · {} tokens · ", model::fmt_tokens(totals.tokens)), NORMAL);
        w.put(range_text(s.start, app.now), DIM);
        w.end();
        w.end();
        for (i, t) in Tool::ALL.into_iter().filter(|&t| app.scan.visible(t)).enumerate() {
            let sel = i == app.selected;
            let total = s.totals(Some(t));
            w.put(format!(" {} ", if sel { "›" } else { " " }), ACCENT);
            w.padded(t.name(), 13, false, if sel { ACCENT } else { BOLD });
            w.padded(&output::cost_text(total), 9, true, NORMAL);
            w.put(" ", NORMAL);
            w.padded(&model::fmt_tokens(total.tokens), 7, true, NORMAL);
            w.end();
            if sel && app.expanded {
                for r in s.rows.iter().filter(|r| r.tool == t) {
                    w.put("     ", DIM);
                    w.padded(&safe(&r.model), 25, false, DIM);
                    w.padded(&r.cost_usd.map_or("?".into(), model::fmt_cost), 9, true, DIM);
                    w.put(" ", DIM);
                    w.padded(&model::fmt_tokens(r.tokens), 7, true, DIM);
                    w.end();
                }
            }
            let five = app.scan.limits.iter().any(|l| l.tool == t && l.window == Window::FiveHour);
            if t.estimates() && !five {
                w.put("     5h  no active block", DIM);
                w.end();
            }
            for l in app.scan.limits.iter().filter(|l| l.tool == t) {
                limit_line(&mut w, l, app.now);
            }
            w.end();
        }
        w.put(format!("  {}", status(app)), DIM);
        w.end();
    }
    if let Some(note) = &app.note {
        w.put(format!("  {}", safe(note)), YELLOW);
        w.end();
    }
    if app.confirm {
        w.put("  Fetch limits from Anthropic, OpenAI and Command Code? y/n", BOLD);
        w.end();
    }
    w.lines
}
