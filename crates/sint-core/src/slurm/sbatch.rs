//! `sbatch` submission.
//!
//! Ports script lines 634-683: submit with `--output=/dev/null
//! --error=/dev/null`, capture stdout and stderr separately, scrape the job id
//! from the trailing integer of stdout, echo the passthrough args as a hint on
//! failure.

use super::{Slurm, SlurmError};

/// Extract the job id from `sbatch` stdout ("Submitted batch job 12345" or
/// `--parsable` "12345" / "12345;cluster").
pub fn parse_job_id(_stdout: &str) -> Option<u64> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

impl Slurm {
    /// Submit `script` with `args` (sbatch options first, then the script and
    /// its arguments). Returns the job id.
    pub fn sbatch(&self, _args: &[String]) -> Result<u64, SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }
}
