//! `scontrol` — job comment tagging, reservations, cluster config.

use super::{Slurm, SlurmError};

/// One reservation from `scontrol show reservation -o`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    pub name: String,
    pub start_epoch: i64,
    pub end_epoch: i64,
    pub flags: Vec<String>,
    pub nodes: String,
    pub users: String,
}

/// Parse `scontrol show reservation -o` (one `Key=Value …` line per
/// reservation). Timestamps are local time `YYYY-MM-DDTHH:MM:SS`.
pub fn parse_reservations(_output: &str) -> Result<Vec<Reservation>, SlurmError> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

impl Slurm {
    /// `scontrol update JobId=ID Comment=…`.
    pub fn set_comment(&self, _job_id: u64, _comment: &str) -> Result<(), SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }

    pub fn reservations(&self) -> Result<Vec<Reservation>, SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }

    /// `scontrol show config` → `ClusterName`, or `None` when unavailable.
    pub fn cluster_name(&self) -> Option<String> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }
}
