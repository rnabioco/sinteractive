//! `~/.cache/sinteractive/` state files.
//!
//! | File | Purpose |
//! |---|---|
//! | `<jobid>.json` | time-budget snapshot, **frozen schema and field order** |
//! | `<jobid>.poke` | touch to force the in-session loop to re-query now |
//! | `<jobid>.notices` | see [`crate::notices`] |
//! | `quota.json` | see [`crate::quota`] |
//! | `<jobid>.metrics.json` | phase 3: latest host snapshot |
//! | `<jobid>.events.ndjson` | phase 3: event log |
//!
//! Every write is write-to-`.tmp`-then-rename so reads are atomic.
//!
//! Honesty contract: `<jobid>.json` is written only when the deadline was
//! confirmed against Slurm just now; if Slurm is unreachable the file is left
//! alone so it ages truthfully. Consumers age it as
//! `remaining_seconds - (now - updated_epoch)` and treat > 120 s as stale.
//! **Never add a key ending in `name` after `name`** — the 0.x bash completion
//! parses it with a greedy regex.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StateFile {
    pub job_id: u64,
    pub name: Option<String>,
    pub node: String,
    pub end_epoch: Option<i64>,
    pub remaining_seconds: Option<i64>,
    pub updated_epoch: i64,
}

/// Seconds after which a state file is stale.
pub const STALE_AFTER: i64 = 120;

impl StateFile {
    /// `remaining_seconds - (now - updated_epoch)`, clamped ≥ 0; `None` when
    /// stale or when the file carries no deadline.
    pub fn aged_remaining(&self, _now: i64) -> Option<i64> {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
}

/// Paths under the cache dir.
#[derive(Debug, Clone)]
pub struct StateDir(pub PathBuf);

impl StateDir {
    pub fn state_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.json"))
    }
    pub fn poke_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.poke"))
    }
    pub fn notices_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.notices"))
    }
    pub fn quota_file(&self) -> PathBuf {
        self.0.join("quota.json")
    }
    pub fn metrics_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.metrics.json"))
    }
    pub fn events_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.events.ndjson"))
    }

    pub fn read_state(&self, _job_id: u64) -> Option<StateFile> {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
    pub fn write_state(&self, _s: &StateFile) -> std::io::Result<()> {
        // TODO(phase-1/agent-B): atomic_write(serde_json::to_string(s) + "\n")
        unimplemented!()
    }
    /// Touch `<jobid>.poke`.
    pub fn poke(&self, _job_id: u64) -> std::io::Result<()> {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
    /// Consume a poke: returns true (and removes the file) if one was pending.
    pub fn take_poke(&self, _job_id: u64) -> bool {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
    /// Poke every `<jobid>.json` present (skips `quota`). Used by `quota --check`.
    pub fn poke_all(&self) -> std::io::Result<()> {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
    /// Remove every per-job file for `job_id` (json, tmp, poke, notices, metrics, events).
    pub fn cleanup(&self, _job_id: u64) {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
    /// Job ids that have a state file (for completion and `poke_all`).
    pub fn known_job_ids(&self) -> Vec<u64> {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
}

/// Write `contents` to `path` via a sibling `.tmp` and rename. Creates the
/// parent directory if needed.
pub fn atomic_write(_path: &Path, _contents: &[u8]) -> std::io::Result<()> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}
