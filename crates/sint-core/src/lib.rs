//! Core library for `sinteractive`.
//!
//! Everything that is not the CLI surface lives here so it can be unit-tested
//! natively and reused by the MCP server, the Claude statusline, and the
//! zellij plugin's data feed. Modules are organised by the contract they own:
//!
//! - [`config`]   — `SINTERACTIVE_*` environment, cache/share dirs, colour mode
//! - [`time`]     — walltime parsing and formatting (`8h`, `1d12h`, `D-HH:MM:SS`)
//! - [`slurm`]    — running and parsing `squeue`/`sbatch`/`scontrol`/`sacct`/`sinfo`
//! - [`session`]  — the session JSON contract and discovery by the `sinteractive[:NAME]` Comment marker
//! - [`state`]    — `~/.cache/sinteractive/<jobid>.json`, `.poke`, atomic writes
//! - [`notices`]  — `<jobid>.notices` (TSV `kind\ttext`) and its producers
//! - [`quota`]    — Bodhi quota daemons and `quota.json`
//! - [`maint`]    — maintenance reservations and fitting a request before one
//! - [`metrics`]  — host snapshots: CPU, memory, GPUs, processes, cgroup-scoped
//! - [`joblimit`] — per-QOS concurrent job cap check
//! - [`theme`]    — Claude Code palette, dark/light aware
//!
//! Design rules carried over from the 0.x bash tool (see `docs/scripting.md`):
//! never restamp an unverified snapshot ("age honestly"); cache the structure
//! never the weather; report allocation size but never export it to the
//! session environment; optional subsystems fail silently.

pub mod color;
pub mod config;
pub mod joblimit;
pub mod maint;
pub mod metrics;
pub mod notices;
pub mod quota;
pub mod session;
pub mod slurm;
pub mod state;
pub mod theme;
pub mod time;

/// Current Unix time in seconds.
pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
