// Used by the commands wired in the next step; the allow goes away with them.
#![allow(dead_code)]

//! Helpers shared by the subcommands: configuration, Slurm handle, palette,
//! JSON output, and target resolution. Keep this small; anything with real
//! logic belongs in `sint-core` where it can be unit-tested.

use anyhow::{anyhow, Result};
use sint_core::color::Palette;
use sint_core::config::{ColorMode, Config};
use sint_core::session::{resolve_target, sessions_only, Target};
use sint_core::slurm::squeue::JobRow;
use sint_core::slurm::Slurm;
use sint_core::state::StateDir;

/// Everything a command needs, built once at dispatch.
pub struct Ctx {
    pub cfg: Config,
    pub slurm: Slurm,
    pub state: StateDir,
}

impl Ctx {
    pub fn new() -> Self {
        let cfg = Config::from_env();
        let state = StateDir(cfg.cache_dir.clone());
        Ctx {
            cfg,
            slurm: Slurm::new(),
            state,
        }
    }

    /// Palette for stdout (reports) or stderr (narration).
    pub fn palette(&self, fd: i32) -> Palette {
        Palette::for_fd(ColorMode::from_env(), fd)
    }

    /// The user's RUNNING+PENDING sinteractive sessions.
    pub fn sessions(&self) -> Result<Vec<JobRow>> {
        let rows = self.slurm.my_jobs(&["RUNNING", "PENDING"])?;
        Ok(sessions_only(&rows).into_iter().cloned().collect())
    }

    /// Resolve an optional CLI target: explicit JOBID/NAME, else the current
    /// session (`SINTERACTIVE_JOB_ID`), else an error naming the fix.
    pub fn resolve(&self, target: Option<&str>) -> Result<u64> {
        match target {
            Some(t) => {
                let t = Target::parse(t);
                if let Target::JobId(id) = t {
                    return Ok(id);
                }
                let rows = self.sessions()?;
                resolve_target(&t, &rows).map_err(|e| anyhow!(e))
            }
            None => self
                .cfg
                .job_id
                .ok_or_else(|| anyhow!("no target given and not inside an sinteractive session")),
        }
    }
}

impl Default for Ctx {
    fn default() -> Self {
        Self::new()
    }
}

/// Print a JSON value on one line, as every `--json` path does.
pub fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}
