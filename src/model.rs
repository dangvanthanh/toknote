use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tool {
    Claude,
    Openai,
    Commandcode,
    Opencode,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Claude, Tool::Openai, Tool::Commandcode, Tool::Opencode];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Claude => "Claude",
            Tool::Openai => "OpenAI",
            Tool::Commandcode => "Command Code",
            Tool::Opencode => "OpenCode",
        }
    }

    /// JSON, cache and redaction identifier.
    pub fn id(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Openai => "openai",
            Tool::Commandcode => "commandcode",
            Tool::Opencode => "opencode",
        }
    }

    #[cfg(feature = "live")]
    pub fn from_id(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id() == s)
    }

    pub fn estimates(self) -> bool {
        matches!(self, Tool::Claude | Tool::Commandcode)
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Window {
    FiveHour,
    Week,
}

impl Window {
    pub fn label(self) -> &'static str {
        match self {
            Window::FiveHour => "5h",
            Window::Week => "7d",
        }
    }

    pub fn length(self) -> i64 {
        match self {
            Window::FiveHour => 18_000 * 1_000_000,
            Window::Week => 604_800 * 1_000_000,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Log,
    Est,
    Live,
}

impl Origin {
    pub fn id(self) -> &'static str {
        match self {
            Origin::Log => "log",
            Origin::Est => "est",
            Origin::Live => "live",
        }
    }

    #[cfg(feature = "live")]
    pub fn from_id(s: &str) -> Option<Origin> {
        [Origin::Log, Origin::Est, Origin::Live].into_iter().find(|o| o.id() == s)
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub tool: Tool,
    pub ts: i64,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cost_usd: Option<f64>,
}

impl Event {
    pub fn new(tool: Tool, ts: i64, model: String) -> Event {
        Event { tool, ts, model, input: 0, output: 0, cache_read: 0, cache_write_5m: 0, cache_write_1h: 0, cost_usd: None }
    }

    pub fn tokens(&self) -> u64 {
        self.input.saturating_add(self.output).saturating_add(self.cache_read).saturating_add(self.cache_write_5m).saturating_add(self.cache_write_1h)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limit {
    pub tool: Tool,
    pub window: Window,
    pub used_pct: Option<f32>,
    pub resets_at: Option<i64>,
    pub origin: Origin,
    pub block_tokens: Option<u64>,
    pub fetched_at: Option<i64>,
}

impl Limit {
    pub fn new(tool: Tool, window: Window, origin: Origin) -> Limit {
        Limit { tool, window, used_pct: None, resets_at: None, origin, block_tokens: None, fetched_at: None }
    }

    pub fn expire(mut self, now: i64) -> Limit {
        if self.resets_at.is_some_and(|t| t <= now) {
            self.used_pct = Some(0.0);
            self.resets_at = None;
        }
        self
    }
}

#[derive(Clone, Debug)]
pub struct Skipped {
    pub path: PathBuf,
    pub count: usize,
}

#[derive(Default, Debug)]
pub struct Scan {
    pub events: Vec<Event>,
    pub limits: Vec<Limit>,
    pub skipped: Vec<Skipped>,
    pub found: [bool; 4],
}

impl Scan {
    pub fn skipped_total(&self) -> usize {
        self.skipped.iter().map(|x| x.count).sum()
    }

    pub fn visible(&self, t: Tool) -> bool {
        self.found[t.index()] || self.limits.iter().any(|l| l.tool == t)
    }

    pub fn any_found(&self) -> bool {
        self.found.contains(&true)
    }
}

#[derive(Default, Debug)]
pub struct LiveResult {
    pub limits: Vec<Limit>,
    pub errors: Vec<String>,
}

/// `prec` fraction digits, rounding the shortest round-trip decimal half up: `2.15` gives `2.2`
/// and `82.5` gives `83`. (`{:.N}` would round the binary value half to even instead.)
pub fn fixed(x: impl std::fmt::Display, prec: usize) -> String {
    let s = x.to_string();
    let (sign, s) = s.strip_prefix('-').map_or(("", s.as_str()), |rest| ("-", rest));
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes().chain(std::iter::repeat(b'0')).take(prec)).collect();
    if frac.as_bytes().get(prec).is_some_and(|&d| d >= b'5') {
        match digits.iter().rposition(|&d| d != b'9') {
            Some(i) => {
                digits[i] += 1;
                digits[i + 1..].fill(b'0');
            }
            None => {
                digits.fill(b'0');
                digits.insert(0, b'1');
            }
        }
    }
    let (int, frac) = digits.split_at(digits.len() - prec);
    let (int, frac) = (String::from_utf8_lossy(int), String::from_utf8_lossy(frac));
    if prec == 0 { format!("{sign}{int}") } else { format!("{sign}{int}.{frac}") }
}

pub fn fmt_tokens(n: u64) -> String {
    let x = n as f64;
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{}K", fixed(x / 1e3, 0)),
        1_000_000..1_000_000_000 => format!("{}M", fixed(x / 1e6, 1)),
        _ => format!("{}B", fixed(x / 1e9, 1)),
    }
}

pub fn fmt_cost(n: f64) -> String {
    format!("${}", fixed(n, 2))
}

/// Replaces each tool's offline limits with its live ones.
pub fn apply_live(s: &mut Scan, r: LiveResult) {
    s.limits.retain(|old| !r.limits.iter().any(|l| l.tool == old.tool));
    s.limits.extend(r.limits);
}
