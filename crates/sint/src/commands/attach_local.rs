//! `sinteractive __attach SESSION` — the node side of `attach`: runs with a
//! tty (`srun --overlap --pty …` or `ssh -X -t NODE …`) and execs the
//! embedded zellij client against the job's headless server (0.x
//! `attach_tmux`, script line 2977).
//!
//! The client attaches with the `config.kdl` the server was started with
//! (`__job` leaves its path in the socket dir): mouse mode is a client-side
//! setting and the bundle id depends on it. Falls back to the bundle for
//! `SINTERACTIVE_MOUSE` when the marker is missing.
//!
//! Not ported: forwarding `DISPLAY`. 0.x did `tmux setenv DISPLAY` so panes
//! opened *after* an `ssh -X` attach could reach the forwarded X server.
//! zellij panes inherit the *server's* environment, fixed when `__job`
//! started it; a later client's `DISPLAY` is not visible to existing shells
//! and zellij has no session-environment store to update for new ones.
//! Users who need X11 in a pane can `export DISPLAY=…` there themselves.

use std::os::unix::process::CommandExt;
use std::process::Stdio;

use anyhow::{anyhow, Result};
use sint_core::config::Config;

use crate::bundle;
use crate::zellij_cmd::{self, ZellijEnv};

/// `sinteractive-<jobid>` → `jobid`.
pub fn job_id_of(session: &str) -> Option<u64> {
    session.strip_prefix("sinteractive-")?.parse::<u64>().ok()
}

pub fn run(session: &str) -> Result<i32> {
    let Some(job_id) = job_id_of(session) else {
        eprintln!("sinteractive: session {session} not found on this node");
        return Ok(1);
    };
    let cfg = Config::from_env();
    let zellij = ZellijEnv::new(&cfg, job_id)?;

    // Is the server up? (0.x: tmux has-session via the exec's own failure.)
    let listed = zellij
        .command(["list-sessions", "--no-formatting"])
        .stdin(Stdio::null())
        .output()
        .map(|o| {
            o.status.success()
                && super::job::session_listed(&String::from_utf8_lossy(&o.stdout), session)
        })
        .unwrap_or(false);
    if !listed {
        eprintln!("sinteractive: session {session} not found on this node");
        return Ok(1);
    }

    let config = match std::fs::read_to_string(zellij_cmd::config_marker(job_id)) {
        Ok(p) if !p.trim().is_empty() && std::path::Path::new(p.trim()).exists() => {
            std::path::PathBuf::from(p.trim())
        }
        _ => bundle::ensure(&cfg, cfg.mouse)?.config,
    };
    let mut cmd = zellij.command(["--config", &config.to_string_lossy(), "attach", session]);
    // zellij refuses to `attach` when ZELLIJ_SESSION_NAME names the target.
    cmd.env_remove("ZELLIJ_SESSION_NAME");
    // exec only returns on failure.
    let e = cmd.exec();
    Err(anyhow!("could not exec {:?}: {e}", cmd.get_program()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_names() {
        assert_eq!(job_id_of("sinteractive-4242"), Some(4242));
        assert_eq!(job_id_of("sinteractive-"), None);
        assert_eq!(job_id_of("sinteractive-x"), None);
        assert_eq!(job_id_of("4242"), None);
        assert_eq!(job_id_of("other-4242"), None);
    }
}
