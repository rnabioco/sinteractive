//! Running and parsing Slurm commands.
//!
//! All Slurm interaction goes through [`Slurm`], which spawns the real
//! binaries found on `PATH`. Tests substitute `tests/fake-slurm/` shims by
//! prepending to `PATH`; the parsers in the submodules are pure functions of
//! command output and are unit-tested directly.
//!
//! Rule inherited from 0.x: whenever a Comment can appear in the output, use
//! pipe-delimited `-o '%i|%k|…'` rather than `--Format` fixed-width columns,
//! which a long comment can shift.

pub mod sacct;
pub mod sbatch;
pub mod scontrol;
pub mod sinfo;
pub mod squeue;

use std::process::Command;

/// Error from running a Slurm command.
#[derive(Debug, thiserror::Error)]
pub enum SlurmError {
    #[error("{cmd} not found on PATH")]
    NotFound { cmd: String },
    #[error("{cmd} failed ({status}): {stderr}")]
    Failed {
        cmd: String,
        status: i32,
        stderr: String,
    },
    #[error("could not parse {cmd} output: {reason}")]
    Parse { cmd: String, reason: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Handle for invoking Slurm binaries.
#[derive(Debug, Clone, Default)]
pub struct Slurm {
    /// Directory holding the binaries; `None` = search `PATH`.
    pub bin_dir: Option<std::path::PathBuf>,
}

impl Slurm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a `Command` for `name`, honouring `bin_dir`.
    pub fn command(&self, name: &str) -> Command {
        match &self.bin_dir {
            Some(d) => Command::new(d.join(name)),
            None => Command::new(name),
        }
    }

    /// Run `name args…`, returning trimmed stdout; maps non-zero exit and
    /// ENOENT to [`SlurmError`].
    pub fn run(&self, name: &str, args: &[&str]) -> Result<String, SlurmError> {
        let out = self.command(name).args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SlurmError::NotFound {
                    cmd: name.to_string(),
                }
            } else {
                SlurmError::Io(e)
            }
        })?;
        if !out.status.success() {
            return Err(SlurmError::Failed {
                cmd: name.to_string(),
                status: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }
}
