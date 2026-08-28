//! `squeue` queries and parsers.
//!
//! Ports the calls at script lines 691-748 (pending wait), 750 (batchhost),
//! 921-1000 (`--list`), 1012-1140 (`--status`), 1140-1236 (agent context),
//! 2152 (`resolve_session_jobid`), 2431 (`refresh_end_epoch`).

use super::{Slurm, SlurmError};

/// One row of `squeue --me -o '%i|%k|%N|%P|%M|%l|%e|%C|%m|%b|%T|%r|%S'`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobRow {
    pub job_id: u64,
    pub comment: String,
    pub node: String,
    pub partition: String,
    pub elapsed: String,
    pub time_limit: String,
    /// Raw `%e` (`N/A`/`Unknown` when no scheduled end).
    pub end_time: String,
    pub cpus: Option<u32>,
    /// Raw `%m` (`32G`, `4000M`, `N/A`).
    pub min_memory: String,
    /// Raw `%b` TRES-per-node (`gres:gpu:2`, `gres:gpu:a100:2`, `N/A`).
    pub tres_per_node: String,
    pub state: String,
    pub reason: String,
    /// Raw `%S` estimated start.
    pub start_time: String,
}

/// The `-o` format string that produces [`JobRow`]. Keep in one place.
pub const JOB_ROW_FORMAT: &str = "%i|%k|%N|%P|%M|%l|%e|%C|%m|%b|%T|%r|%S";

/// Parse the pipe-delimited rows. Blank lines are skipped; a row with too
/// few fields is an error naming the line.
pub fn parse_job_rows(_output: &str) -> Result<Vec<JobRow>, SlurmError> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// `gres:gpu:2` / `gres:gpu:a100:2` / `gpu:2` → 2; `N/A`/empty → 0. Always an
/// integer: "no GPUs is a fact, not a gap".
pub fn gpus_from_tres(_tres: &str) -> u32 {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// `32G` → 32768, `4000M` → 4000, `1T` → 1048576; `N/A`/unparseable → None.
pub fn mem_to_mb(_mem: &str) -> Option<u64> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

impl Slurm {
    /// All of the user's jobs in `states` (e.g. `["RUNNING"]`), as rows.
    pub fn my_jobs(&self, _states: &[&str]) -> Result<Vec<JobRow>, SlurmError> {
        // TODO(phase-1/agent-A): squeue --me --states … --noheader -o JOB_ROW_FORMAT
        unimplemented!()
    }

    /// One job by id (any state). `Ok(None)` when squeue no longer lists it.
    pub fn job(&self, _job_id: u64) -> Result<Option<JobRow>, SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }

    /// `squeue --jobs ID --Format batchhost` → node name.
    pub fn batch_host(&self, _job_id: u64) -> Result<Option<String>, SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }
}
