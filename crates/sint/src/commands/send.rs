//! `sinteractive send TARGET COMMAND` — type a command into a session's
//! shell and press Enter, over one ssh (`write-chars`, then `write 13`).
//! This is the user's live shell, so callers only do it when asked.

use anyhow::Result;

use super::common::{eprint_error, ssh_batch, Ctx};
use crate::cli::SendArgs;
use crate::zellij_cmd::ZellijEnv;

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
    let env = ZellijEnv::new(&ctx.cfg, session.job_id)?;
    let remote = remote_command(&env, &args.command);
    let out = ssh_batch(&session.node, 10, &remote).output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let detail = stderr
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("ssh exited {}", out.status));
        eprint_error(
            &p,
            &format!(
                "could not send to session {} on {}: {detail}",
                session.job_id, session.node
            ),
        );
        return Ok(1);
    }
    eprintln!("{}✓{} sent to session {}", p.ok, p.reset, session.job_id);
    Ok(0)
}

/// Both zellij actions in one remote shell command: the keystrokes, then
/// Enter (`13`), the second only if the first got through.
fn remote_command(env: &ZellijEnv, command: &str) -> String {
    let chars = env
        .remote_argv(["action", "write-chars", "-p", "terminal_0", command])
        .join(" ");
    let enter = env
        .remote_argv(["action", "write", "-p", "terminal_0", "13"])
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
            exe: PathBuf::from("/opt/sinteractive"),
        };
        let cmd = remote_command(&env, "echo 'hi there'");
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
