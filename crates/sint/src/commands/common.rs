//! Helpers shared by the subcommands: configuration, Slurm handle, palette,
//! JSON output, and target resolution. Keep this small; anything with real
//! logic belongs in `sint-core` where it can be unit-tested.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use sint_core::color::Palette;
use sint_core::config::{ColorMode, Config};
use sint_core::session::{resolve_target, sessions_only, SessionInfo, Target};
use sint_core::slurm::squeue::JobRow;
use sint_core::slurm::Slurm;
use sint_core::state::StateDir;
use sint_core::time::format_short_duration;

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

    /// The user's RUNNING sinteractive sessions.
    pub fn running_sessions(&self) -> Result<Vec<JobRow>> {
        let rows = self.slurm.my_jobs(&["RUNNING"])?;
        Ok(sessions_only(&rows).into_iter().cloned().collect())
    }

    /// Whether this process runs inside an sinteractive session (the
    /// session exports `SINTERACTIVE_JOB_ID`). Attaching from inside one
    /// would nest multiplexers, so launch-and-attach and `attach` refuse.
    pub fn inside_session(&self) -> bool {
        self.cfg.job_id.is_some()
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

/// The running binary, to hand to `sbatch`/`srun`/`ssh` as the script that
/// runs the internal verbs (`__job`, `__attach`) — the 0.x `"$0"`.
pub fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow!("cannot locate the sinteractive binary: {e}"))
}

/// The human `status` block (script line 1103): title line, then
/// `Partition:`/`Resources:`/`Elapsed:` and, for a RUNNING job, `Remaining:`
/// coloured by how much is left. Notices are the caller's to append.
pub fn render_status(info: &SessionInfo, p: &Palette) -> String {
    let (reset, bold, dim, id, key) = (&p.reset, &p.bold, &p.dim, &p.id, &p.key);
    // Green only for RUNNING; PENDING and every terminal state are things
    // you would want to notice, so they share the warning colour.
    let state_c = if info.state == "RUNNING" {
        &p.ok
    } else {
        &p.warn
    };
    let mut out = String::new();
    let mut title = format!("{bold}Session{reset} {id}{}{reset}", info.job_id);
    if let Some(name) = &info.name {
        title.push_str(&format!(" {bold}({name}){reset}"));
    }
    let on = match &info.node {
        Some(node) => format!(" {dim}on{reset} {id}{node}{reset}"),
        None => String::new(),
    };
    out.push_str(&format!("{title}: {state_c}{}{reset}{on}\n", info.state));
    let field = |label: &str| format!("  {key}{label:<11}{reset} ");
    out.push_str(&format!(
        "{}{}\n",
        field("Partition:"),
        info.partition.as_deref().unwrap_or("")
    ));
    let mut res = format!(
        "{} CPUs, {}",
        info.cpus.map(|c| c.to_string()).unwrap_or_default(),
        info.memory.as_deref().unwrap_or("")
    );
    if info.gpus > 0 {
        res.push_str(&format!(", {} GPU", info.gpus));
    }
    if info.gpus > 1 {
        res.push('s');
    }
    out.push_str(&format!("{}{res}\n", field("Resources:")));
    out.push_str(&format!(
        "{}{} {dim}(limit {}){reset}\n",
        field("Elapsed:"),
        info.elapsed.as_deref().unwrap_or(""),
        info.time_limit.as_deref().unwrap_or("")
    ));
    if let Some(remaining) = info.remaining_seconds {
        // The one number worth reading at a glance, so it is coloured by
        // how much of it is left rather than left the same shade all session.
        let rem_c = if remaining < 900 {
            &p.err
        } else if remaining < 3600 {
            &p.warn
        } else {
            &p.ok
        };
        out.push_str(&format!(
            "{}{rem_c}{}{reset}\n",
            field("Remaining:"),
            format_short_duration(remaining)
        ));
    }
    out
}

/// One row of the "other sessions" tables (script lines 826, 848): the
/// header when `row` is `None`.
pub fn session_table_line(row: Option<&JobRow>, p: &Palette) -> String {
    match row {
        None => format!(
            "  {}{:<10}  {:<14}  {:<10}  {:<10}{}",
            p.dim, "JOBID", "NODE", "ELAPSED", "TIMELIMIT", p.reset
        ),
        Some(r) => format!(
            "  {}{:<10}{}  {:<14}  {:<10}  {:<10}",
            p.id, r.job_id, p.reset, r.node, r.elapsed, r.time_limit
        ),
    }
}
