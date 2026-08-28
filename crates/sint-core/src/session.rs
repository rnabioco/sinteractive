//! The session JSON contract and session discovery.
//!
//! **Frozen contract** (docs/scripting.md). Field order is the serialisation
//! order below. `cwd` appears only from `list`; `created` only from `ensure`.
//! `end_epoch` and `remaining_seconds` go `null` together. `gpus` is always an
//! integer. `cpus` is what Slurm *allocated*, which can exceed the request.
//!
//! Identity: a session is a Slurm job whose Comment is exactly `sinteractive`
//! or `sinteractive:NAME`. The job Name (`sint-NAME`) is decorative.

use serde::{Deserialize, Serialize};

use crate::slurm::squeue::JobRow;

/// Comment marker prefix.
pub const COMMENT_BASE: &str = "sinteractive";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionInfo {
    pub job_id: u64,
    pub name: Option<String>,
    pub state: String,
    pub node: Option<String>,
    pub partition: Option<String>,
    pub cpus: Option<u32>,
    pub memory: Option<String>,
    pub memory_mb: Option<u64>,
    pub gpus: u32,
    pub time_limit: Option<String>,
    pub elapsed: Option<String>,
    pub end_epoch: Option<i64>,
    pub remaining_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<bool>,
}

impl SessionInfo {
    /// The `{"job_id":N,"state":"NOT_FOUND"}` object (exit 1 at the CLI).
    pub fn not_found(job_id: u64) -> serde_json::Value {
        serde_json::json!({"job_id": job_id, "state": "NOT_FOUND"})
    }

    /// Build from an `squeue` row at time `now`.
    pub fn from_row(_row: &JobRow, _now: i64) -> Self {
        // TODO(phase-1/agent-A): name from comment, mem_to_mb, gpus_from_tres,
        // end_epoch via time::slurm_timestamp_to_epoch, remaining clamped ≥ 0.
        unimplemented!()
    }
}

/// `sinteractive` / `sinteractive:NAME` → `Some(None)` / `Some(Some(NAME))`;
/// any other comment → `None`.
pub fn parse_comment(_comment: &str) -> Option<Option<String>> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// Comment for a session with an optional name.
pub fn comment_for(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{COMMENT_BASE}:{n}"),
        None => COMMENT_BASE.to_string(),
    }
}

/// Session names: `^[A-Za-z0-9._-]+$`.
pub fn validate_name(_name: &str) -> Result<(), String> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// A target given on the command line: a job id or a session name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    JobId(u64),
    Name(String),
}

impl Target {
    pub fn parse(s: &str) -> Target {
        match s.parse::<u64>() {
            Ok(id) => Target::JobId(id),
            Err(_) => Target::Name(s.to_string()),
        }
    }
}

/// Resolve a target to a job id against `rows` (RUNNING+PENDING sessions).
/// Errors on zero or more than one name match (script line 2152).
pub fn resolve_target(_target: &Target, _rows: &[JobRow]) -> Result<u64, String> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// Filter rows to sinteractive sessions.
pub fn sessions_only(rows: &[JobRow]) -> Vec<&JobRow> {
    rows.iter()
        .filter(|r| parse_comment(&r.comment).is_some())
        .collect()
}
