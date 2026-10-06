use crate::model::Event;

// Prices as of 2026-10-05, USD per 1M tokens: input, output, cache read, 5m write, 1h write.
// Claude: platform.claude.com/docs/en/about-claude/pricing (standard 1M context).
// OpenAI: developers.openai.com/api/docs/pricing, Standard, short context.
const PRICES: &[(&str, [f64; 5])] = &[
    ("claude-fable-5-1", [10.0, 50.0, 0.25, 12.5, 20.0]),
    ("claude-opus-5-5", [4.0, 20.0, 0.20, 5.0, 8.0]),
    ("claude-sonnet-5-5", [2.0, 10.0, 0.20, 2.5, 4.0]),
    ("claude-haiku-4-5", [1.0, 5.0, 0.10, 1.25, 2.0]),
    ("claude-opus-5", [5.0, 25.0, 0.50, 6.25, 10.0]),
    ("claude-sonnet-5", [2.0, 10.0, 0.20, 2.5, 4.0]),
    ("claude-opus-4-8", [5.0, 25.0, 0.50, 6.25, 10.0]),
    ("claude-opus-4-7", [5.0, 25.0, 0.50, 6.25, 10.0]),
    ("claude-opus-4-6", [5.0, 25.0, 0.50, 6.25, 10.0]),
    ("claude-opus-4-5", [5.0, 25.0, 0.50, 6.25, 10.0]),
    ("claude-sonnet-4-6", [3.0, 15.0, 0.30, 3.75, 6.0]),
    ("claude-sonnet-4-5", [3.0, 15.0, 0.30, 3.75, 6.0]),
    ("gpt-6-astra", [10.0, 50.0, 1.0, 12.5, 12.5]),
    ("gpt-6.1-sol", [2.0, 10.0, 0.10, 2.5, 2.5]),
    ("gpt-6-sol", [2.0, 10.0, 0.20, 2.5, 2.5]),
    ("gpt-6-luna", [0.10, 0.50, 0.01, 0.125, 0.125]),
    ("gpt-5.6-sol", [4.0, 20.0, 0.40, 5.0, 5.0]),
];

/// Recorded cost, else the table price of the normalized model name (`[...]` suffix and a
/// trailing `-YYYYMMDD` stripped). Unknown models: `None`.
pub fn cost(e: &Event) -> Option<f64> {
    if e.cost_usd.is_some() {
        return e.cost_usd;
    }
    let mut base = e.model.split('[').next().unwrap_or_default();
    if let Some((head, tail)) = base.rsplit_once('-')
        && tail.len() == 8
        && tail.bytes().all(|b| b.is_ascii_digit())
    {
        base = head;
    }
    let (_, values) = PRICES.iter().find(|(model, _)| *model == base)?;
    let counts = [e.input, e.output, e.cache_read, e.cache_write_5m, e.cache_write_1h];
    let sum: f64 = counts.iter().zip(values).map(|(&n, v)| n as f64 * v).sum();
    Some(sum / 1e6)
}
