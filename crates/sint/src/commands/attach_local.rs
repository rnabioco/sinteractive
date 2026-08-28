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
//! Attaching detaches whatever was attached before — see
//! [`detach_other_clients`].
//!
//! Not ported: forwarding `DISPLAY`. 0.x did `tmux setenv DISPLAY` so panes
//! opened *after* an `ssh -X` attach could reach the forwarded X server.
//! zellij panes inherit the *server's* environment, fixed when `__job`
//! started it; a later client's `DISPLAY` is not visible to existing shells
//! and zellij has no session-environment store to update for new ones.
//! Users who need X11 in a pane can `export DISPLAY=…` there themselves.

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{anyhow, Result};
use sint_core::color::Palette;
use sint_core::config::{ColorMode, Config};
use zellij_utils::consts::{ipc_connect, CLIENT_SERVER_CONTRACT_DIR};
use zellij_utils::data::ClientId;
use zellij_utils::ipc::{ClientToServerMsg, IpcSenderWithContext};

use crate::bundle;
use crate::zellij_cmd::{self, ZellijEnv};

/// `sinteractive-<jobid>` → `jobid`.
pub fn job_id_of(session: &str) -> Option<u64> {
    session.strip_prefix("sinteractive-")?.parse::<u64>().ok()
}

/// The socket the session's server listens on: the tmux `-L` directory, the
/// client/server contract directory zellij interposes, then the session name.
/// Built from [`ZellijEnv`] rather than `zellij_utils::consts::ZELLIJ_SOCK_DIR`
/// because that reads `ZELLIJ_SOCKET_DIR` from our own environment, which we
/// set on the client we spawn, not on ourselves.
fn socket_path(zellij: &ZellijEnv, session: &str) -> PathBuf {
    zellij
        .socket_dir
        .join(&*CLIENT_SERVER_CONTRACT_DIR)
        .join(session)
}

/// The client ids in `zellij action list-clients` output.
///
/// It prints a header row and then one row per client, the id first; rows
/// whose first word is not a number (the header) are skipped.
pub fn parse_client_ids(stdout: &str) -> Vec<ClientId> {
    stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next()?.parse::<ClientId>().ok())
        .collect()
}

/// The clients zellij currently has attached to `session`.
fn attached_clients(zellij: &ZellijEnv, session: &str) -> Vec<ClientId> {
    let Ok(out) = zellij
        .command(["--session", session, "action", "list-clients"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_client_ids(&String::from_utf8_lossy(&out.stdout))
}

/// Detach every client already attached to `session`, and say how many.
///
/// zellij sizes a tab to the *smallest* attached client in each dimension
/// (`Screen::recompute_tab_size`), so one leftover client — a dropped ssh
/// whose client process outlived it, an attach from a narrower window —
/// shrinks the session for everyone: content stops short of the right edge
/// and the status bar floats above the bottom of the terminal. 0.x never
/// showed this because tmux's default `window-size latest` sizes to the
/// newest client instead. zellij has no such option, so we do what
/// `tmux attach -d` does and make the newcomer the only client. Detaching
/// is what the kicked client sees (`ExitReason::Normal`), and removing it
/// re-runs the size computation, so the new client gets the full terminal.
fn detach_other_clients(zellij: &ZellijEnv, session: &str) -> usize {
    let client_ids = attached_clients(zellij, session);
    if client_ids.is_empty() {
        return 0;
    }
    let n = client_ids.len();
    // Our own connection, unlike `ClientOsApi::connect_to_server`, must not
    // retry forever: a server that went away between the two calls would
    // hang the attach.
    let Ok(sock) = ipc_connect(&socket_path(zellij, session)) else {
        return 0;
    };
    let mut sender: IpcSenderWithContext<ClientToServerMsg> = IpcSenderWithContext::new(sock);
    match sender.send_client_msg(ClientToServerMsg::DetachSession { client_ids }) {
        Ok(()) => n,
        Err(_) => 0,
    }
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
    // Take the session over before attaching, so zellij sizes it to this
    // terminal alone.
    let detached = detach_other_clients(&zellij, session);
    if detached > 0 {
        let p = Palette::for_fd(ColorMode::from_env(), 2);
        let s = if detached == 1 { "" } else { "s" };
        eprintln!(
            "{}Detached {detached} other client{s} from this session.{}",
            p.dim, p.reset
        );
    }

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
    fn client_ids_skip_the_header() {
        let out = "CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND\n\
                   1         terminal_0     claude         \n\
                   2         terminal_0     bash           \n";
        assert_eq!(parse_client_ids(out), vec![1, 2]);
    }

    #[test]
    fn no_clients_is_empty() {
        assert_eq!(parse_client_ids(""), Vec::<ClientId>::new());
        assert_eq!(
            parse_client_ids("CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND\n"),
            Vec::<ClientId>::new()
        );
    }

    #[test]
    fn socket_is_the_path_the_server_listens_on() {
        let cfg = Config::from_env();
        let zellij = ZellijEnv::new(&cfg, 4242).unwrap();
        let path = socket_path(&zellij, "sinteractive-4242");
        assert!(path.ends_with("sinteractive-4242"));
        assert!(path.starts_with(&zellij.socket_dir));
        assert!(path
            .to_string_lossy()
            .contains(&*CLIENT_SERVER_CONTRACT_DIR));
    }

    #[test]
    fn session_names() {
        assert_eq!(job_id_of("sinteractive-4242"), Some(4242));
        assert_eq!(job_id_of("sinteractive-"), None);
        assert_eq!(job_id_of("sinteractive-x"), None);
        assert_eq!(job_id_of("4242"), None);
        assert_eq!(job_id_of("other-4242"), None);
    }
}
