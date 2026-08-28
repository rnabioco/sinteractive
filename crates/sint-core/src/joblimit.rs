//! Per-QOS concurrent job cap (script lines 2072-2150).
//!
//! 0.x hardcoded partition and QOS `interactive`; here the check runs for
//! whatever QOS the launch resolves to (`--qos`, `SINTERACTIVE_QOS`, or the
//! partition name as a last guess) and fails open when `sacctmgr`/`squeue`
//! are unavailable.

use crate::slurm::squeue::JobRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitHit {
    pub qos: String,
    pub limit: u32,
    /// Jobs counted against the limit (RUNNING + PENDING in that partition/QOS).
    pub jobs: Vec<JobRow>,
}

/// `Some(hit)` when submitting one more job would exceed `limit`.
pub fn check(
    _qos: &str,
    _limit: Option<u32>,
    _rows: &[JobRow],
    _partition: &str,
) -> Option<LimitHit> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}
