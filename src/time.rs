//! Microsecond UTC timestamps; local calendar math through chrono's system-timezone `Local`.
use chrono::{DateTime, Datelike, Local, LocalResult, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Utc};
use serde_json::Value;

pub const SECOND: i64 = 1_000_000;
pub const DAY: i64 = 86_400 * SECOND;

pub fn now() -> i64 {
    Utc::now().timestamp_micros()
}

pub fn local(t: i64) -> DateTime<Local> {
    DateTime::from_timestamp_micros(t).unwrap_or_default().with_timezone(&Local)
}

/// Local midnight `offset` calendar days from `t`'s day. Ambiguous times take the earlier
/// instant; a midnight skipped by DST moves forward, as `mktime` with `tm_isdst = -1` does.
pub fn midnight(t: i64, offset: i64) -> i64 {
    let day = local(t).date_naive() + TimeDelta::days(offset);
    let naive = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    to_local(naive).map_or(t, |d| d.timestamp() * SECOND)
}

fn to_local(naive: NaiveDateTime) -> Option<DateTime<Local>> {
    (0..=4).find_map(|half_hours| match Local.from_local_datetime(&(naive + TimeDelta::minutes(30 * half_hours))) {
        LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => Some(d),
        LocalResult::None => None,
    })
}

pub fn format(t: i64, f: &str) -> String {
    local(t).format(f).to_string()
}

pub fn reset(t: i64, n: i64) -> String {
    // Count calendar days, not elapsed seconds across DST transitions.
    let days = (local(t).date_naive() - local(n).date_naive()).num_days();
    format(
        t,
        if days <= 0 {
            "%H:%M"
        } else if days <= 6 {
            "%a %H:%M"
        } else {
            "%b %-d %H:%M"
        },
    )
}

fn number(s: &[u8]) -> Option<i32> {
    if !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse().ok()
}

/// Strict RFC 3339 (`T`, `t` or space separator; `Z` or `±HH:MM`), sub-microsecond digits truncated.
pub fn parse(s: &str) -> Option<i64> {
    let s = s.as_bytes();
    if s.len() < 20 || s[4] != b'-' || s[7] != b'-' || !matches!(s[10], b'T' | b't' | b' ') || s[13] != b':' || s[16] != b':' {
        return None;
    }
    let y = number(&s[0..4])?;
    let mo = number(&s[5..7])?;
    let d = number(&s[8..10])?;
    let h = number(&s[11..13])?;
    let mi = number(&s[14..16])?;
    let se = number(&s[17..19])?;
    if h > 23 || mi > 59 || se > 59 {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(y, u32::try_from(mo).ok()?, u32::try_from(d).ok()?)?;
    let epoch = date.and_hms_opt(h as u32, mi as u32, se as u32)?.and_utc().timestamp();
    let mut i = 19;
    let mut fraction: i64 = 0;
    if i < s.len() && s[i] == b'.' {
        i += 1;
        let start = i;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
        for j in 0..6 {
            fraction = fraction * 10 + if start + j < i { i64::from(s[start + j] - b'0') } else { 0 };
        }
    }
    if i >= s.len() {
        return None;
    }
    let mut offset: i64 = 0;
    if s[i] == b'Z' || s[i] == b'z' {
        if i + 1 != s.len() {
            return None;
        }
    } else {
        if (s[i] != b'+' && s[i] != b'-') || s.len() != i + 6 || s[i + 3] != b':' {
            return None;
        }
        let oh = i64::from(number(&s[i + 1..i + 3])?);
        let om = i64::from(number(&s[i + 4..i + 6])?);
        if oh > 23 || om > 59 {
            return None;
        }
        offset = (oh * 3600 + om * 60) * if s[i] == b'+' { 1 } else { -1 };
    }
    Some((epoch - offset) * SECOND + fraction)
}

/// RFC 3339 UTC with 0, 3 or 6 fractional digits.
pub fn rfc(t: i64) -> String {
    let secs = t.div_euclid(SECOND);
    let f = t.rem_euclid(SECOND);
    let base = DateTime::from_timestamp(secs, 0).unwrap_or_default().format("%Y-%m-%dT%H:%M:%S");
    if f == 0 {
        format!("{base}Z")
    } else if f % 1000 == 0 {
        format!("{base}.{:03}Z", f / 1000)
    } else {
        format!("{base}.{f:06}Z")
    }
}

/// RFC 3339 string, or unix seconds / milliseconds as a number.
pub fn parse_value(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => parse(s),
        Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                return if n > 1_000_000_000_000 { n.checked_mul(1000) } else { n.checked_mul(SECOND) };
            }
            if !n.is_f64() {
                return None;
            }
            let n = n.as_f64()?;
            if !n.is_finite() || !(-9e12..=9e15).contains(&n) {
                return None;
            }
            (n as i64).checked_mul(if n > 1e12 { 1000 } else { SECOND })
        }
        _ => None,
    }
}

/// Local calendar fields used by the TUI and period starts.
pub fn weekday_from_monday(t: i64) -> i64 {
    i64::from(local(t).weekday().num_days_from_monday())
}

pub fn day_of_month(t: i64) -> i64 {
    i64::from(local(t).day())
}
