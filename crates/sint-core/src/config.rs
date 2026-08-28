//! `SINTERACTIVE_*` environment and well-known paths.
//!
//! Explicit CLI flags always win over these; these win over built-in
//! defaults. Defaults match Bodhi (the origin cluster); Alpine users override
//! `partition`/`qos` in their shell profile as documented in the README.

use std::path::PathBuf;

/// Resolved configuration. Build with [`Config::from_env`].
#[derive(Debug, Clone)]
pub struct Config {
    /// `SINTERACTIVE_TIME` — default walltime in Slurm form (`24:00:00`).
    pub time: String,
    /// `SINTERACTIVE_PARTITION` — default partition (`interactive`).
    pub partition: String,
    /// `SINTERACTIVE_QOS` — added as `--qos` only when set.
    pub qos: Option<String>,
    /// `SINTERACTIVE_CPUS` — default `--cpus-per-task` (2).
    pub cpus: u32,
    /// `SINTERACTIVE_MEM` — default `--mem` (`8G`).
    pub mem: String,
    /// `SINTERACTIVE_MOUSE` — `on/1/true/yes` enables mouse mode.
    pub mouse: bool,
    /// `SINTERACTIVE_ZELLIJ` — path to a zellij binary; bypasses the bundle.
    pub zellij: Option<PathBuf>,
    /// `SINTERACTIVE_CACHE` — state dir; default `~/.cache/sinteractive`.
    pub cache_dir: PathBuf,
    /// `SINTERACTIVE_SHARE` — asset root override for `install-claude`.
    pub share_dir: Option<PathBuf>,
    /// `SINTERACTIVE_WARN_YELLOW` (3600) / `SINTERACTIVE_WARN_RED` (600) /
    /// `SINTERACTIVE_GRACE` (10) / `SINTERACTIVE_POLL` (30, floor 5).
    pub warn_yellow: i64,
    pub warn_red: i64,
    pub grace: i64,
    pub poll: i64,
    /// `SINTERACTIVE_QUOTA_POLL` (600, floor 30), `_FILE`, `_HOSTS`, `_PORT`, `_TIMEOUT` (5).
    pub quota_poll: i64,
    pub quota_file: PathBuf,
    pub quota_hosts: Vec<String>,
    pub quota_port: u16,
    pub quota_timeout: u64,
    /// `SINTERACTIVE_AGENT_WARN` (1800) — walltime-guard hook threshold.
    pub agent_warn: i64,
    /// `SINTERACTIVE_JOB_ID` / `SINTERACTIVE_NAME` — set inside a session.
    pub job_id: Option<u64>,
    pub name: Option<String>,
}

impl Config {
    /// Read every `SINTERACTIVE_*` variable, applying defaults and floors.
    /// Never fails: an unparseable value falls back to the default.
    pub fn from_env() -> Self {
        // TODO(phase-1/agent-B): implement per the table in docs/usage.md and
        // the bash script lines 19, 55, 220, 545-578, 1274-1276, 1334,
        // 2474-2487, 2842-2845. Default quota hosts are 172.20.8.110..=118.
        unimplemented!("Config::from_env")
    }
}

/// `SINTERACTIVE_COLOR` — `auto` (default), `always`, `never`; `NO_COLOR` honoured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    pub fn from_env() -> Self {
        // TODO(phase-1/agent-B)
        ColorMode::Auto
    }
}
