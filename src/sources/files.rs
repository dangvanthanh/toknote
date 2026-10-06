use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use memchr::memmem::Finder;
use serde::de::DeserializeOwned;
use walkdir::WalkDir;

use crate::model::{Event, Scan, Skipped};

/// One file's events, parsed on a worker thread. `merge` applies them on the caller in
/// discovery order, so results are identical to a sequential scan.
#[derive(Debug, Default)]
pub struct Parsed {
    pub path: PathBuf,
    pub events: Vec<Event>,
    /// Cross-file dedup key per event (`None`: not deduped). Empty when the tool has no keys.
    pub keys: Vec<Option<String>>,
    pub skipped: usize,
}

impl Parsed {
    pub fn new(path: &Path) -> Parsed {
        Parsed { path: path.to_path_buf(), ..Parsed::default() }
    }
}

/// Keeps the first event per key across files.
pub fn merge(s: &mut Scan, seen: &mut HashSet<String>, r: Parsed) {
    let mut keys = r.keys.into_iter();
    for e in r.events {
        if let Some(Some(k)) = keys.next()
            && !seen.insert(k)
        {
            continue;
        }
        s.events.push(e);
    }
    if r.skipped > 0 {
        s.skipped.push(Skipped { path: r.path, count: r.skipped });
    }
}

/// Regular `<prefix>*.jsonl` files below `root`, depth-first in directory order. Symlinks inside
/// the tree are not followed. A missing root yields nothing.
pub fn jsonl(root: &Path, prefix: &str) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .map_while(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(prefix) && n.ends_with(".jsonl")))
        .map(walkdir::DirEntry::into_path)
        .collect()
}

/// File size for scheduling; 0 when unknown.
pub fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

/// Whole small file, or `None` if unreadable or larger than `cap`.
#[cfg(feature = "live")]
pub fn read(path: &Path, cap: u64) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    File::open(path).ok()?.take(cap + 1).read_to_end(&mut data).ok()?;
    (data.len() as u64 <= cap).then_some(data)
}

/// Usage-only parse: unknown fields are skipped, and only declared (usage) fields are kept.
pub fn parse<T: DeserializeOwned>(line: &[u8]) -> serde_json::Result<T> {
    serde_json::from_slice(line)
}

pub fn contains(line: &[u8], needle: &[u8]) -> bool {
    memchr::memmem::find(line, needle).is_some()
}

/// The file couldn't be read completely; callers drop it.
#[derive(Debug)]
pub struct ReadFailed;

const NONE: usize = usize::MAX;
const CHUNK: usize = 1 << 20;
const CAP: usize = 1 << 30;

/// Lines containing any of `needles` (at most two), in order, each once, streamed through one
/// reused buffer. Memory is bounded by the longest line rather than the file size. Each needle is
/// searched over the whole buffer instead of splitting every line first: most log bytes never
/// match, and `memmem` is SIMD-accelerated.
pub struct FileLines {
    file: File,
    finders: Vec<Finder<'static>>,
    buf: Vec<u8>,
    /// Bytes read into `buf`.
    end: usize,
    /// `buf[..data]` holds only complete lines (or everything at EOF).
    data: usize,
    eof: bool,
    /// Always a line start inside `buf[..data]`.
    pos: usize,
    /// Next match per needle at or after its last search position.
    hits: [usize; 2],
}

impl FileLines {
    pub fn open(path: &Path, needles: &[&'static [u8]]) -> Option<FileLines> {
        assert!(needles.len() <= 2);
        let file = File::open(path).ok()?;
        let finders = needles.iter().map(|n| Finder::new(*n)).collect();
        Some(FileLines { file, finders, buf: vec![0; CHUNK], end: 0, data: 0, eof: false, pos: 0, hits: [NONE; 2] })
    }

    /// The next matching line; valid until the next call.
    pub fn next(&mut self) -> Result<Option<&[u8]>, ReadFailed> {
        loop {
            if let Some((start, end)) = self.next_range() {
                return Ok(Some(&self.buf[start..end]));
            }
            if self.eof {
                return Ok(None);
            }
            self.fill()?;
        }
    }

    fn next_range(&mut self) -> Option<(usize, usize)> {
        let data = &self.buf[..self.data];
        let mut hit = NONE;
        for (finder, h) in self.finders.iter().zip(&mut self.hits) {
            if *h < self.pos {
                *h = data.get(self.pos..).and_then(|d| finder.find(d)).map_or(NONE, |i| self.pos + i);
            }
            hit = hit.min(*h);
        }
        if hit == NONE {
            return None;
        }
        let start = memchr::memrchr(b'\n', &data[self.pos..hit]).map_or(self.pos, |i| self.pos + i + 1);
        let end = memchr::memchr(b'\n', &data[hit..]).map_or(data.len(), |i| hit + i);
        self.pos = end + 1;
        Some((start, end))
    }

    /// Keeps the incomplete last line, then reads until at least one more line is complete.
    fn fill(&mut self) -> Result<(), ReadFailed> {
        self.buf.copy_within(self.data..self.end, 0);
        self.end -= self.data;
        let mut complete = None;
        while complete.is_none() {
            if self.end == self.buf.len() {
                if self.buf.len() >= CAP {
                    return Err(ReadFailed);
                }
                self.buf.resize(self.buf.len() * 2, 0);
            }
            let n = match self.file.read(&mut self.buf[self.end..]) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(ReadFailed),
            };
            if let Some(i) = memchr::memrchr(b'\n', &self.buf[self.end..self.end + n]) {
                complete = Some(self.end + i + 1);
            }
            self.end += n;
        }
        self.data = complete.unwrap_or(self.end);
        self.pos = 0;
        let data = &self.buf[..self.data];
        for (finder, h) in self.finders.iter().zip(&mut self.hits) {
            *h = finder.find(data).unwrap_or(NONE);
        }
        Ok(())
    }
}
