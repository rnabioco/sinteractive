//! `sinteractive send TARGET COMMAND [--pane P]` — type a command into a
//! session's shell and press Enter, over one ssh (`write-chars`, then `write 13`).
//! This is the user's live shell, so callers only do it when asked.

use anyhow::{anyhow, Result};

use super::common::{eprint_error, ssh_batch, Ctx, RunningSession};
use super::pane::resolve_pane;
use super::peek::first_stderr_line;
use crate::cli::SendArgs;
use crate::zellij_cmd::ZellijEnv;

/// Type `command` into a pane (`None`: the session's shell) and press Enter,
/// over one ssh. The error names the session, the node and what ssh or
/// zellij said.
pub fn send_command(
    ctx: &Ctx,
    session: &RunningSession,
    command: &str,
    pane: Option<&str>,
) -> Result<()> {
    let env = ZellijEnv::new(&ctx.cfg, session.job_id)?;
    let pane = resolve_pane(ctx, session, pane)?;
    let remote = remote_command(&env, command, &pane);
    let out = ssh_batch(&session.node, 10, &remote).output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "could not send to session {} on {}: {}",
            session.job_id,
            session.node,
            first_stderr_line(&out)
        ));
    }
    Ok(())
}

pub fn run(args: SendArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let p = ctx.palette(2);
    if args.command.trim().is_empty() {
        eprint_error(&p, "nothing to send: COMMAND is empty");
        return Ok(2);
    }
    let Some(session) = ctx.resolve_running(&args.target)? else {
        return Ok(1);
    };
    if let Err(e) = send_command(&ctx, &session, &args.command, args.pane.as_deref()) {
        eprint_error(&p, &format!("{e:#}"));
        return Ok(1);
    }
    eprintln!("{}✓{} sent to session {}", p.ok, p.reset, session.job_id);
    Ok(0)
}

/// Both zellij actions in one remote shell command: the keystrokes, then
/// Enter (`13`), the second only if the first got through.
fn remote_command(env: &ZellijEnv, command: &str, pane: &str) -> String {
    let chars = env
        .remote_argv(["action", "write-chars", "-p", pane, command])
        .join(" ");
    let enter = env
        .remote_argv(["action", "write", "-p", pane, "13"])
        .join(" ");
    format!("{chars} && {enter}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn remote_command_quotes_and_chains() {
        let env = ZellijEnv {
            job_id: 7,
            socket_dir: PathBuf::from("/tmp/sint-7"),
            xdg_cache_home: PathBuf::from("/c/xdg"),
            cache_dir: PathBuf::from("/c"),
            exe: PathBuf::from("/opt/sinteractive"),
        };
        let cmd = remote_command(&env, "echo 'hi there'", "terminal_0");
        assert!(
            cmd.contains("zellij action write-chars -p terminal_0 'echo '\\''hi there'\\'''"),
            "{cmd}"
        );
        assert!(cmd.contains(" && env "), "{cmd}");
        assert!(
            cmd.ends_with("zellij action write -p terminal_0 13"),
            "{cmd}"
        );
        assert_eq!(cmd.matches("ZELLIJ_SOCKET_DIR=/tmp/sint-7").count(), 2);
    }
}
