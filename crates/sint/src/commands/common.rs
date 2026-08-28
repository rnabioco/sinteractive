//! Helpers shared by the subcommands: configuration, Slurm handle, palette,
//! JSON output, and target resolution. Keep this small; anything with real
//! logic belongs in `sint-core` where it can be unit-tested.

use anyhow::{anyhow, Result};
use sint_core::color::Palette;
use sint_core::config::{ColorMode, Config};
use sint_core::session::{resolve_target, sessions_only, Target};
use sint_core::slurm::squeue::JobRow;
use sint_core::slurm::{Slurm, SlurmError};
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

    /// [`Ctx::resolve`], reporting a no-match or ambiguous name on stderr in
    /// the 0.x wording (script line 2152) and returning `Ok(None)`; the
    /// caller then exits 1. A Slurm failure is still an `Err`.
    pub fn resolve_reporting(&self, target: Option<&str>) -> Result<Option<u64>> {
        match self.resolve(target) {
            Ok(id) => Ok(Some(id)),
            Err(e) if e.downcast_ref::<SlurmError>().is_some() => Err(e),
            Err(e) => {
                let p = self.palette(2);
                let msg = e.to_string();
                let ambiguous = msg.starts_with("multiple ");
                match msg.rsplit_once(": ").filter(|_| ambiguous) {
                    Some((head, ids)) => {
                        eprint_error(&p, &format!("{head}:"));
                        eprintln!("  {}{ids}{}", p.id, p.reset);
                        eprintln!("{}Specify a JOBID instead.{}", p.dim, p.reset);
                    }
                    None => {
                        eprint_error(&p, &msg);
                        eprintln!(
                            "{}Run 'sinteractive list' to see available sessions.{}",
                            p.dim, p.reset
                        );
                    }
                }
                Ok(None)
            }
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

/// `sinteractive: MSG` in the error colour on stderr — the 0.x
/// `${C_ERR}${C_BOLD}${PROG}:${C_RST}${C_ERR} …${C_RST}` line.
pub fn eprint_error(p: &Palette, msg: &str) {
    eprintln!(
        "{}{}sinteractive:{}{} {msg}{}",
        p.err, p.bold, p.reset, p.err, p.reset
    );
}

/// `N CPUs, MEM[, G GPU[s]]` — the resources line shared by `status` and
/// `agent-context` (script lines 1090-1093, 1163-1167).
pub fn resources_line(row: &JobRow) -> String {
    let cpus = row.cpus.map(|c| c.to_string()).unwrap_or_default();
    let mut res = format!("{cpus} CPUs, {}", row.min_memory);
    let gpus = sint_core::slurm::squeue::gpus_from_tres(&row.tres_per_node);
    if gpus > 0 {
        res.push_str(&format!(", {gpus} GPU"));
    }
    if gpus > 1 {
        res.push('s');
    }
    res
}
