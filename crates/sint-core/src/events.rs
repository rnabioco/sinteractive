//! `<jobid>.events.ndjson` — the per-session event log.
//!
//! The in-session loop (`sinteractive __job`) appends one line per event;
//! `sinteractive events` prints or tails it and the MCP server's
//! `wait_for_event` polls it. Each line is one JSON object:
//!
//! ```json
//! {"ts":1783152195,"kind":"walltime_warn","remaining":1800}
//! ```
//!
//! `ts` (epoch seconds) and `kind` are always present; every other key is
//! event-specific and flattened into the same object. The kinds the loop
//! emits, with their extra fields:
//!
//! | kind | fields | when |
//! |---|---|---|
//! | `started` | `job`, `node`, `name` | the loop's first tick |
//! | `walltime_warn` | `remaining` | ≤ 30 min left (once) |
//! | `walltime_red` | `remaining` | ≤ 10 min left (once) |
//! | `quota_over` | `over_kb`, `hard_kb` | storage quota exceeded (once per episode) |
//! | `job_done` | `job`, `name` | one of the user's *other* jobs left the queue |
//! | `gpu_idle` | `gpu`, `util_pct`, `idle_secs` | a held GPU under 5 % for 10 min (once per episode) |
//! | `ended` | `job`, `reason` | the session is over (`walltime`, `gone`, `signal`) |
//!
//! Appends are `O_APPEND` writes of a single line, so concurrent readers
//! never see a torn record; the file is removed with the other per-job
//! state at teardown ([`StateDir::cleanup`]).

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::state::StateDir;

/// One event line. `fields` holds every key other than `ts` and `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Epoch seconds.
    pub ts: i64,
    pub kind: String,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

impl Event {
    /// An event of `kind` stamped with the current time.
    pub fn new(kind: impl Into<String>) -> Self {
        Self::at(crate::now_epoch(), kind)
    }

    /// An event of `kind` stamped `ts` (the loop passes its own clock).
    pub fn at(ts: i64, kind: impl Into<String>) -> Self {
        Event {
            ts,
            kind: kind.into(),
            fields: Map::new(),
        }
    }

    /// Add (or replace) one extra field.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// One extra field as a string, when present and a string.
    pub fn field_str(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(Value::as_str)
    }

    /// One extra field as an integer, when present and numeric.
    pub fn field_i64(&self, key: &str) -> Option<i64> {
        self.fields.get(key).and_then(Value::as_i64)
    }

    /// The NDJSON line, without the trailing newline.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Parse one line; `None` for blank or malformed input.
    pub fn parse_line(line: &str) -> Option<Event> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        serde_json::from_str(line).ok()
    }
}

/// Append `event` as one line to `<jobid>.events.ndjson`, creating the
/// directory and file as needed.
pub fn append(dir: &StateDir, job_id: u64, event: &Event) -> io::Result<()> {
    let path = dir.events_file(job_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut line = event.to_line();
    line.push('\n');
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(line.as_bytes())
}

/// Every parseable event in the file, in file order. A missing file is an
/// empty log, not an error.
pub fn read_all(dir: &StateDir, job_id: u64) -> io::Result<Vec<Event>> {
    let file = match fs::File::open(dir.events_file(job_id)) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        if let Some(ev) = Event::parse_line(&line?) {
            out.push(ev);
        }
    }
    Ok(out)
}

/// The events with `ts > after_ts`, in file order.
pub fn read_since(dir: &StateDir, job_id: u64, after_ts: i64) -> io::Result<Vec<Event>> {
    let mut all = read_all(dir, job_id)?;
    all.retain(|e| e.ts > after_ts);
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let sd = StateDir(dir.path().join("cache"));
        assert_eq!(read_all(&sd, 7).unwrap(), Vec::new(), "missing = empty");

        let started = Event::at(100, "started")
            .with("job", 7)
            .with("node", "n1")
            .with("name", "web");
        let warn = Event::at(200, "walltime_warn").with("remaining", 1800);
        append(&sd, 7, &started).unwrap();
        append(&sd, 7, &warn).unwrap();

        let text = fs::read_to_string(sd.events_file(7)).unwrap();
        assert_eq!(
            text,
            "{\"ts\":100,\"kind\":\"started\",\"job\":7,\"node\":\"n1\",\"name\":\"web\"}\n\
             {\"ts\":200,\"kind\":\"walltime_warn\",\"remaining\":1800}\n"
        );
        assert_eq!(
            read_all(&sd, 7).unwrap(),
            vec![started.clone(), warn.clone()]
        );
        assert_eq!(read_since(&sd, 7, 100).unwrap(), vec![warn.clone()]);
        assert_eq!(read_since(&sd, 7, 99).unwrap().len(), 2);
        assert!(read_since(&sd, 7, 200).unwrap().is_empty());

        let ev = &read_all(&sd, 7).unwrap()[0];
        assert_eq!(ev.field_str("node"), Some("n1"));
        assert_eq!(ev.field_i64("job"), Some(7));
        assert_eq!(ev.field_str("job"), None);
        assert_eq!(warn.field_i64("remaining"), Some(1800));
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let sd = StateDir(dir.path().to_path_buf());
        fs::write(
            sd.events_file(3),
            "{\"ts\":1,\"kind\":\"started\"}\n\nnot json\n{\"kind\":\"no ts\"}\n{\"ts\":2,\"kind\":\"ended\",\"reason\":\"gone\"}\n",
        )
        .unwrap();
        let evs = read_all(&sd, 3).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].kind, "started");
        assert!(evs[0].fields.is_empty());
        assert_eq!(evs[1].field_str("reason"), Some("gone"));
        assert_eq!(Event::parse_line("  "), None);
    }

    #[test]
    fn new_stamps_now_and_with_replaces() {
        let e = Event::new("x").with("a", 1).with("a", 2);
        assert!(e.ts > 1_700_000_000);
        assert_eq!(e.field_i64("a"), Some(2));
        assert_eq!(e.fields.len(), 1);
    }
}
