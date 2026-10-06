use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Row};

use crate::ctx::Ctx;
use crate::model::{Event, Scan, Skipped, Tool};

/// Only usage fields leave SQLite; the `part` table (message text) is never read.
const QUERY: &str = "SELECT time_created,
       json_extract(data, '$.modelID'),
       json_extract(data, '$.tokens.input'),
       json_extract(data, '$.tokens.output'),
       json_extract(data, '$.tokens.reasoning'),
       json_extract(data, '$.tokens.cache.read'),
       json_extract(data, '$.tokens.cache.write'),
       json_extract(data, '$.cost')
FROM message WHERE json_extract(data, '$.role') = 'assistant'";

/// Opens, queries and closes the database on the calling thread. An unreadable database
/// contributes no events and one skipped entry.
pub fn scan(ctx: &Ctx) -> Scan {
    let mut s = Scan::default();
    let path = ctx.data_home.join("opencode/opencode.db");
    if !path.is_file() {
        return s;
    }
    s.found[Tool::Opencode.index()] = true;
    match read(&path) {
        Some(events) => s.events = events,
        None => s.skipped.push(Skipped { path, count: 1 }),
    }
    s
}

/// Non-negative integer counter; `NULL` is 0, anything else invalidates the database.
fn count(row: &Row, i: usize) -> Option<u64> {
    match row.get_ref(i).ok()? {
        ValueRef::Null => Some(0),
        ValueRef::Integer(n) => Some(n.max(0) as u64),
        _ => None,
    }
}

fn read(path: &Path) -> Option<Vec<Event>> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    let mut stmt = db.prepare(QUERY).ok()?;
    let mut rows = stmt.query([]).ok()?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().ok()? {
        let ValueRef::Integer(created) = row.get_ref(0).ok()? else { return None };
        let ts = created.checked_mul(1000)?;
        let name = match row.get_ref(1).ok()? {
            ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
            ValueRef::Null => "unknown".into(),
            _ => return None,
        };
        let model = name.rsplit('/').next().unwrap_or_default().to_owned();
        let cost = match row.get_ref(7).ok()? {
            ValueRef::Null => None,
            ValueRef::Real(x) => Some(x),
            ValueRef::Integer(n) => Some(n as f64),
            _ => return None,
        };
        events.push(Event {
            input: count(row, 2)?,
            output: count(row, 3)?.saturating_add(count(row, 4)?),
            cache_read: count(row, 5)?,
            cache_write_5m: count(row, 6)?,
            cost_usd: cost,
            ..Event::new(Tool::Opencode, ts, model)
        });
    }
    Some(events)
}
