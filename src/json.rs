//! Usage-only JSON shapes. Field order is declaration order (alphabetical, as before).
use serde::{Serialize, Serializer};
use serde_json::value::RawValue;

use crate::model::Limit;
use crate::time;

/// Shortest round-trip decimal, always with a fraction or exponent (`1.0`, `0.0123`);
/// non-finite values are `null`.
pub struct Num(pub Option<f64>);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Some(x) if x.is_finite() => {
                let mut text = x.to_string();
                if !text.contains(['.', 'e', 'E']) {
                    text.push_str(".0");
                }
                RawValue::from_string(text).map_err(serde::ser::Error::custom)?.serialize(s)
            }
            _ => s.serialize_none(),
        }
    }
}

pub fn timestamp(t: Option<i64>) -> Option<String> {
    t.map(time::rfc)
}

#[derive(Serialize)]
pub struct LimitJson {
    pub block_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    pub origin: &'static str,
    pub resets_at: Option<String>,
    pub tool: &'static str,
    pub used_pct: Num,
    pub window: &'static str,
}

impl From<&Limit> for LimitJson {
    fn from(l: &Limit) -> LimitJson {
        LimitJson {
            block_tokens: l.block_tokens,
            fetched_at: timestamp(l.fetched_at),
            origin: l.origin.id(),
            resets_at: timestamp(l.resets_at),
            tool: l.tool.id(),
            used_pct: Num(l.used_pct.map(f64::from)),
            window: l.window.label(),
        }
    }
}
