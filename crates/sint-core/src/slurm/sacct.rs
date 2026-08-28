//! `sacct` / `sacctmgr` — accounting history and QOS limits.

use super::{Slurm, SlurmError};

/// One completed/failed job from `sacct -X -P -n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountedJob {
    pub job_id: String,
    pub name: String,
    pub partition: String,
    pub state: String,
    pub elapsed: String,
    pub req_mem: String,
    pub max_rss: String,
    pub alloc_cpus: Option<u32>,
    pub end_epoch: Option<i64>,
}

pub const SACCT_FORMAT: &str = "JobID,JobName,Partition,State,Elapsed,ReqMem,MaxRSS,AllocCPUS,End";

pub fn parse_sacct(_output: &str) -> Result<Vec<AccountedJob>, SlurmError> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

impl Slurm {
    /// Recent jobs for the user since `since` (`now-1day` style).
    pub fn recent_jobs(&self, _since: &str) -> Result<Vec<AccountedJob>, SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }

    /// `sacctmgr -nP show qos NAME format=MaxJobsPerUser` → limit, `None` when
    /// unset or unavailable (callers fail open).
    pub fn qos_max_jobs_per_user(&self, _qos: &str) -> Option<u32> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }
}
