//! `sinfo` — node lists for `doctor --nodes`.

use super::{Slurm, SlurmError};

impl Slurm {
    /// `sinfo -hN -o %N | sort -u`.
    pub fn node_names(&self) -> Result<Vec<String>, SlurmError> {
        // TODO(phase-1/agent-A)
        unimplemented!()
    }
}
