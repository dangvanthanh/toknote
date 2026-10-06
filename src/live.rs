//! All network access is here; compiled only with the `live` feature.
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ctx::Ctx;
use crate::features::VERSION;
use crate::json::{self, LimitJson};
use crate::model::{Limit, LiveResult, Origin, Tool, Window};
use crate::sources::files;
use crate::time;

const URLS: [&str; 3] =
    ["https://api.anthropic.com/api/oauth/usage", "https://chatgpt.com/backend-api/wham/usage", "https://api.commandcode.ai/alpha/billing/credits"];
const MAX: u64 = 1024 * 1024;

#[derive(Deserialize, Clone)]
struct OAuth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    oauth: OAuth,
}

#[derive(Deserialize)]
struct ClaudeWindow {
    utilization: Option<f32>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    five_hour: Option<ClaudeWindow>,
    seven_day: Option<ClaudeWindow>,
}

#[derive(Deserialize)]
struct OpenAITokens {
    access_token: String,
    account_id: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIAuth {
    auth_mode: Option<String>,
    tokens: Option<OpenAITokens>,
}

#[derive(Deserialize)]
struct OpenAIWindow {
    used_percent: Option<f32>,
    limit_window_seconds: Option<u64>,
    reset_at: Option<i64>,
}

#[derive(Deserialize)]
struct OpenAIRates {
    primary_window: Option<OpenAIWindow>,
    secondary_window: Option<OpenAIWindow>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    rate_limit: Option<OpenAIRates>,
}

#[derive(Deserialize)]
struct CommandAuth {
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct CommandWindow {
    used: f64,
    cap: f64,
    #[serde(rename = "resetAt")]
    reset_at: Option<Value>,
}

#[derive(Deserialize)]
struct CommandLimits {
    limited: Option<bool>,
    #[serde(rename = "fiveHour")]
    five_hour: Option<CommandWindow>,
    weekly: Option<CommandWindow>,
}

#[derive(Deserialize)]
struct CommandUsage {
    #[serde(rename = "windowLimits")]
    window_limits: Option<CommandLimits>,
}

#[derive(Deserialize)]
struct WireLimit {
    tool: String,
    window: String,
    origin: String,
    used_pct: Option<f32>,
    resets_at: Option<String>,
    block_tokens: Option<u64>,
    fetched_at: Option<String>,
}

#[derive(Deserialize)]
struct WireCached {
    fetched_at: String,
    limits: Vec<WireLimit>,
}

#[derive(Deserialize)]
struct WireCache {
    claude: Option<WireCached>,
    openai: Option<WireCached>,
    commandcode: Option<WireCached>,
    opencode: Option<WireCached>,
}

#[derive(Serialize)]
struct CachedJson {
    fetched_at: Option<String>,
    limits: Vec<LimitJson>,
}

#[derive(Serialize, Default)]
struct CacheJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    claude: Option<CachedJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    openai: Option<CachedJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commandcode: Option<CachedJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opencode: Option<CachedJson>,
}

#[derive(Clone)]
struct Cached {
    fetched_at: i64,
    limits: Vec<Limit>,
}

/// No limits and no error: the tool has no local login and is skipped silently.
#[derive(Default)]
struct Reply {
    limits: Vec<Limit>,
    err: Option<String>,
}

fn fail(e: impl Into<String>) -> Reply {
    Reply { limits: Vec::new(), err: Some(e.into()) }
}

struct Http {
    status: u16,
    body: Vec<u8>,
    retry: Option<String>,
}

/// One GET with a 5s deadline. Redirects are returned, never followed, so credential headers can't
/// reach another host; environment proxies are ignored. Error bodies are never read.
fn request(url: &str, headers: &[(&str, String)]) -> Option<Http> {
    if headers.iter().any(|(_, v)| v.contains(['\r', '\n'])) {
        return None;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .https_only(true)
        .proxy(None)
        .user_agent(format!("toknote/{VERSION}"))
        .accept_encoding("identity")
        .build()
        .into();
    let mut req = agent.get(url);
    for (name, value) in headers {
        req = req.header(*name, value.as_str());
    }
    let mut response = req.call().ok()?;
    let status = response.status().as_u16();
    let retry = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_owned);
    if !(200..300).contains(&status) {
        return Some(Http { status, body: Vec::new(), retry });
    }
    let body = response.body_mut().with_config().limit(MAX).read_to_vec().ok()?;
    Some(Http { status, body, retry })
}

fn get(tool: Tool, headers: &[(&str, String)]) -> Result<Vec<u8>, String> {
    let response = request(URLS[tool.index()], headers).ok_or("request failed")?;
    let hint = match tool {
        Tool::Claude => "run claude to refresh login",
        Tool::Openai => "run codex to refresh login",
        _ => "run cmd login",
    };
    match response.status {
        200..=299 => Ok(response.body),
        401 | 403 => Err(format!("HTTP {}; {hint}", response.status)),
        429 => Err(match response.retry {
            Some(retry) => format!("rate limited (HTTP 429, retry in {retry}s)"),
            None => "rate limited (HTTP 429)".into(),
        }),
        status => Err(format!("HTTP {status}")),
    }
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

fn live_limit(tool: Tool, window: Window, pct: f32, reset: Option<i64>) -> Limit {
    Limit { used_pct: Some(pct), resets_at: reset, ..Limit::new(tool, window, Origin::Live) }
}

fn nonempty(limits: Vec<Limit>) -> Reply {
    if limits.is_empty() { fail("no limits in response") } else { Reply { limits, err: None } }
}

/// Keychain item on macOS, with a 5s timeout. Its output holds credentials and is never printed.
#[cfg(target_os = "macos")]
fn keychain() -> Option<Vec<u8>> {
    use std::io::Read as _;
    use std::process::{Command, Stdio};
    use std::time::Instant;
    let mut child = Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        stdout.take(MAX).read_to_end(&mut data).map(|_| data)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    reader.join().ok()?.ok()
}

#[cfg(not(target_os = "macos"))]
fn keychain() -> Option<Vec<u8>> {
    None
}

/// Keychain first, then `~/.claude/.credentials.json`; the first unexpired one wins.
fn credentials(ctx: &Ctx, now: i64) -> Option<OAuth> {
    let parse = |data: Vec<u8>| files::parse::<ClaudeCredentials>(&data).ok().map(|c| c.oauth);
    let options = [keychain().and_then(parse), files::read(&ctx.home.join(".claude/.credentials.json"), MAX).and_then(parse)];
    let fresh = options.iter().flatten().find(|o| o.expires_at.is_none_or(|e| e > now / 1000));
    fresh.or(options.iter().flatten().next()).cloned()
}

fn claude(ctx: &Ctx, now: i64) -> Reply {
    let Some(oauth) = credentials(ctx, now) else { return Reply::default() };
    if oauth.expires_at.is_some_and(|e| e <= now / 1000) {
        return fail("login expired; run claude to refresh");
    }
    let body = match get(Tool::Claude, &[bearer(&oauth.access_token), ("anthropic-beta", "oauth-2025-04-20".into())]) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let Ok(usage) = files::parse::<ClaudeUsage>(&body) else { return fail("unexpected response shape") };
    let mut limits = Vec::new();
    for (window, w) in [(Window::FiveHour, usage.five_hour), (Window::Week, usage.seven_day)] {
        let Some(w) = w else { continue };
        let Some(pct) = w.utilization else { continue };
        let reset = match w.resets_at {
            Some(s) => match time::parse(&s) {
                Some(t) => Some(t),
                None => return fail("unexpected response shape"),
            },
            None => None,
        };
        limits.push(live_limit(Tool::Claude, window, pct, reset));
    }
    nonempty(limits)
}

fn openai(ctx: &Ctx) -> Reply {
    let Some(data) = files::read(&ctx.openai_home.join("auth.json"), MAX) else { return Reply::default() };
    let Ok(auth) = files::parse::<OpenAIAuth>(&data) else { return fail("unreadable auth.json") };
    let Some(tokens) = auth.tokens else { return fail("logged in with an API key; no plan limits") };
    if auth.auth_mode.as_deref().unwrap_or("chatgpt") != "chatgpt" {
        return fail("logged in with an API key; no plan limits");
    }
    let mut headers = vec![bearer(&tokens.access_token)];
    if let Some(id) = tokens.account_id {
        headers.push(("ChatGPT-Account-Id", id));
    }
    let body = match get(Tool::Openai, &headers) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let Ok(usage) = files::parse::<OpenAIUsage>(&body) else { return fail("unexpected response shape") };
    let Some(rl) = usage.rate_limit else { return fail("no limits in response") };
    let mut limits = Vec::new();
    for (i, w) in [rl.primary_window, rl.secondary_window].into_iter().enumerate() {
        let Some(w) = w else { continue };
        let Some(pct) = w.used_percent else { continue };
        let window = match w.limit_window_seconds {
            Some(18000) => Window::FiveHour,
            Some(604800) => Window::Week,
            Some(_) => continue,
            None if i == 0 => Window::FiveHour,
            None => Window::Week,
        };
        limits.push(live_limit(Tool::Openai, window, pct, w.reset_at.and_then(|n| n.checked_mul(time::SECOND))));
    }
    nonempty(limits)
}

/// `$COMMAND_CODE_API_KEY` first, then `~/.commandcode/auth.json`.
fn commandcode(ctx: &Ctx) -> Reply {
    let env_key = std::env::var("COMMAND_CODE_API_KEY").ok().map(|k| k.trim_matches([' ', '\t', '\r', '\n']).to_owned()).filter(|k| !k.is_empty());
    let key = env_key
        .or_else(|| files::read(&ctx.home.join(".commandcode/auth.json"), MAX).and_then(|d| files::parse::<CommandAuth>(&d).ok()).and_then(|a| a.api_key));
    let Some(token) = key else { return Reply::default() };
    let body = match get(Tool::Commandcode, &[bearer(&token)]) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let Ok(usage) = files::parse::<CommandUsage>(&body) else { return fail("unexpected response shape") };
    let Some(wl) = usage.window_limits else { return fail("no plan limits") };
    if wl.limited == Some(false) {
        return fail("no plan limits");
    }
    let mut limits = Vec::new();
    for (window, w) in [(Window::FiveHour, wl.five_hour), (Window::Week, wl.weekly)] {
        let Some(w) = w else { continue };
        let pct = if w.cap > 0.0 { (w.used / w.cap * 100.0).min(100.0) } else { 0.0 };
        limits.push(live_limit(Tool::Commandcode, window, pct as f32, w.reset_at.as_ref().and_then(time::parse_value)));
    }
    nonempty(limits)
}

fn cache_path(ctx: &Ctx) -> std::path::PathBuf {
    ctx.cache_home.join("toknote")
}

/// Per-tool cache entries; any malformed entry invalidates the whole cache.
fn cache_read(ctx: &Ctx) -> [Option<Cached>; 4] {
    let parsed = files::read(&cache_path(ctx).join("live.json"), MAX).and_then(|d| files::parse::<WireCache>(&d).ok());
    let Some(parsed) = parsed else { return Default::default() };
    let mut cache: [Option<Cached>; 4] = Default::default();
    for (i, wc) in [parsed.claude, parsed.openai, parsed.commandcode, parsed.opencode].into_iter().enumerate() {
        let Some(wc) = wc else { continue };
        let Some(entry) = cached(wc) else { return Default::default() };
        cache[i] = Some(entry);
    }
    cache
}

fn cached(wc: WireCached) -> Option<Cached> {
    let fetched_at = time::parse(&wc.fetched_at)?;
    let mut limits = Vec::new();
    for l in wc.limits {
        let window = match l.window.as_str() {
            "5h" => Window::FiveHour,
            "7d" => Window::Week,
            _ => return None,
        };
        let parse_opt = |s: Option<String>| match s {
            Some(s) => time::parse(&s).map(Some),
            None => Some(None),
        };
        limits.push(Limit {
            tool: Tool::from_id(&l.tool)?,
            window,
            origin: Origin::from_id(&l.origin)?,
            used_pct: l.used_pct,
            resets_at: parse_opt(l.resets_at)?,
            block_tokens: l.block_tokens,
            fetched_at: parse_opt(l.fetched_at)?,
        });
    }
    Some(Cached { fetched_at, limits })
}

/// Limits only. Directory 0700, file 0600, written through an exclusive per-process temporary file
/// and an atomic rename.
fn cache_write(ctx: &Ctx, cache: &[Option<Cached>; 4]) -> std::io::Result<()> {
    let dir = cache_path(ctx);
    DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    File::open(&dir)?.set_permissions(fs::Permissions::from_mode(0o700))?;
    let entry = |i: usize| {
        cache[i].as_ref().map(|c| CachedJson { fetched_at: json::timestamp(Some(c.fetched_at)), limits: c.limits.iter().map(LimitJson::from).collect() })
    };
    let doc = CacheJson { claude: entry(0), openai: entry(1), commandcode: entry(2), opencode: entry(3) };
    let bytes = serde_json::to_vec(&doc).map_err(std::io::Error::other)?;
    // Exclusive creation never follows a pre-existing symlink and avoids concurrent clobbers.
    let temp = dir.join(format!("live.{}.{}.tmp", std::process::id(), time::now()));
    let written = write_new(&temp, &bytes).and_then(|()| fs::rename(&temp, dir.join("live.json")));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?.write_all(bytes)
}

/// Each tool independently: fresh cache (<60s) is reused, otherwise fetched; on failure a cache
/// entry under 15 min old is reused with a note.
pub fn fetch(ctx: &Ctx, now: i64) -> LiveResult {
    let mut cache = cache_read(ctx);
    let mut result = LiveResult::default();
    let mut dirty = false;
    for tool in [Tool::Claude, Tool::Openai, Tool::Commandcode] {
        let i = tool.index();
        let cached = cache[i].clone().filter(|v| now - v.fetched_at < 15 * 60 * time::SECOND);
        if let Some(v) = &cached
            && now - v.fetched_at < 60 * time::SECOND
        {
            result.limits.extend(v.limits.iter().map(|l| l.expire(now)));
            continue;
        }
        let reply = match tool {
            Tool::Claude => claude(ctx, now),
            Tool::Openai => openai(ctx),
            _ => commandcode(ctx),
        };
        if let Some(err) = reply.err {
            match cached {
                Some(v) => {
                    let mins = (now - v.fetched_at) / (60 * time::SECOND);
                    result.errors.push(format!("{}: {err}, showing {mins}m old result", tool.id()));
                    result.limits.extend(v.limits.iter().map(|l| l.expire(now)));
                }
                None => result.errors.push(format!("{}: {err}", tool.id())),
            }
        } else if !reply.limits.is_empty() {
            let limits: Vec<Limit> = reply.limits.into_iter().map(|l| Limit { fetched_at: Some(now), ..l }).collect();
            result.limits.extend(limits.iter().copied());
            cache[i] = Some(Cached { fetched_at: now, limits });
            dirty = true;
        }
    }
    if dirty {
        let _ = cache_write(ctx, &cache);
    }
    result
}
