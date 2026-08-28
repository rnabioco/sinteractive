//! How every command talks to the embedded zellij for a given session.
//!
//! One session = one zellij server, isolated by its socket directory (the
//! tmux `-L` equivalent) and named after the Slurm job:
//!
//! ```text
//! ZELLIJ_SOCKET_DIR   $SINTERACTIVE_RUNTIME_DIR|/tmp / sint-<jobid>      (node-local)
//! session name        sinteractive-<jobid>
//! XDG_CACHE_HOME      <sinteractive cache>/xdg                          (shared FS)
//! ready marker        <socket dir>/ready   — written by `__job` once the server is up
//! ```
//!
//! zellij keeps its own caches (plugin artifacts, the permission grants,
//! session info) under `$XDG_CACHE_HOME/zellij`; pointing that at our cache
//! dir keeps the permission pre-grant for the status plugin in one known
//! place and keeps zellij out of `$HOME` on clusters where that is tiny.
//!
//! Local use: [`ZellijEnv::command`] builds `current_exe() zellij ARGS…`
//! with the environment set. Remote use (login node → compute node):
//! [`ZellijEnv::remote_argv`] gives the `env K=V… <exe> zellij ARGS…` words
//! to hand to `ssh NODE`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use sint_core::config::Config;

use crate::commands::common::current_exe;

/// Per-session zellij environment.
#[derive(Debug, Clone)]
pub struct ZellijEnv {
    pub job_id: u64,
    pub socket_dir: PathBuf,
    pub xdg_cache_home: PathBuf,
    pub exe: PathBuf,
}

/// `SINTERACTIVE_RUNTIME_DIR` (default `/tmp`): node-local scratch for the
/// socket dir and the readiness marker.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("SINTERACTIVE_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn session_name(job_id: u64) -> String {
    format!("sinteractive-{job_id}")
}

pub fn socket_dir(job_id: u64) -> PathBuf {
    runtime_dir().join(format!("sint-{job_id}"))
}

pub fn ready_marker(job_id: u64) -> PathBuf {
    socket_dir(job_id).join("ready")
}

/// `<socket dir>/config` — written by `__job` with the path of the
/// `config.kdl` the server was started with, so `__attach` on the same node
/// attaches with the matching bundle (mouse mode is a client-side setting
/// and the bundle id depends on it).
pub fn config_marker(job_id: u64) -> PathBuf {
    socket_dir(job_id).join("config")
}

pub fn xdg_cache_home(cfg: &Config) -> PathBuf {
    cfg.cache_dir.join("xdg")
}

impl ZellijEnv {
    pub fn new(cfg: &Config, job_id: u64) -> Result<Self> {
        Ok(ZellijEnv {
            job_id,
            socket_dir: socket_dir(job_id),
            xdg_cache_home: xdg_cache_home(cfg),
            exe: current_exe()?,
        })
    }

    pub fn session(&self) -> String {
        session_name(self.job_id)
    }

    /// `K=V` pairs every zellij invocation for this session needs.
    pub fn env_pairs(&self) -> Vec<(String, String)> {
        vec![
            (
                "ZELLIJ_SOCKET_DIR".into(),
                self.socket_dir.to_string_lossy().into_owned(),
            ),
            (
                "XDG_CACHE_HOME".into(),
                self.xdg_cache_home.to_string_lossy().into_owned(),
            ),
            ("ZELLIJ_SESSION_NAME".into(), self.session()),
        ]
    }

    /// `current_exe() zellij ARGS…` with the session environment. Variables
    /// a nested zellij must not inherit (`ZELLIJ`, `ZELLIJ_PANE_ID`) are
    /// cleared.
    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut cmd = Command::new(&self.exe);
        cmd.arg("zellij");
        cmd.args(args);
        cmd.env_remove("ZELLIJ").env_remove("ZELLIJ_PANE_ID");
        for (k, v) in self.env_pairs() {
            cmd.env(k, v);
        }
        cmd
    }

    /// Argument words for running the same thing on the node over ssh:
    /// `env K=V… <exe> zellij ARGS…`.
    pub fn remote_argv<I, S>(&self, args: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut v = vec!["env".to_string()];
        for (k, val) in self.env_pairs() {
            v.push(format!("{k}={}", shell_quote(&val)));
        }
        v.push(shell_quote(&self.exe.to_string_lossy()));
        v.push("zellij".into());
        v.extend(args.into_iter().map(|a| shell_quote(a.as_ref())));
        v
    }
}

/// Single-quote `s` for a POSIX shell (ssh runs the remote command through
/// the login shell).
pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./=:@%+,".contains(&b))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Pre-grant the status plugin's permissions in zellij's cache so a headless
/// session never waits on the interactive prompt. The key is the plain wasm
/// path (zellij's `RunPluginLocation::File` display form).
pub fn grant_plugin_permissions(xdg_cache_home: &Path, plugin: &Path) -> std::io::Result<()> {
    let dir = xdg_cache_home.join("zellij");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("permissions.kdl");
    let key = plugin.to_string_lossy();
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    if existing.contains(&format!("\"{key}\"")) {
        return Ok(());
    }
    let block = format!("\"{key}\" {{\n    ReadApplicationState\n    ChangeApplicationState\n}}\n");
    sint_core::state::atomic_write(&file, format!("{existing}{block}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("abc-1.2/x"), "abc-1.2/x");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn names_and_dirs() {
        std::env::remove_var("SINTERACTIVE_RUNTIME_DIR");
        assert_eq!(session_name(42), "sinteractive-42");
        assert_eq!(socket_dir(42), PathBuf::from("/tmp/sint-42"));
        assert_eq!(ready_marker(42), PathBuf::from("/tmp/sint-42/ready"));
        assert_eq!(config_marker(42), PathBuf::from("/tmp/sint-42/config"));
    }

    #[test]
    fn permissions_file_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = Path::new("/x/sint-zellij.wasm");
        grant_plugin_permissions(dir.path(), plugin).unwrap();
        grant_plugin_permissions(dir.path(), plugin).unwrap();
        let text = std::fs::read_to_string(dir.path().join("zellij/permissions.kdl")).unwrap();
        assert_eq!(text.matches("sint-zellij.wasm").count(), 1);
        assert!(text.contains("ChangeApplicationState"));
    }
}
