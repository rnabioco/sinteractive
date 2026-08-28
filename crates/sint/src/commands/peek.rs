//! `sinteractive peek TARGET [-n LINES]` — the last lines of a session's
//! shell pane, read over one ssh with the embedded zellij's `dump-screen`.
//! Replaces the 0.x `ssh NODE tmux capture-pane` recipe the hpc-compute
//! skill used to document.

use anyhow::{anyhow, Result};

use super::common::{eprint_error, ssh_batch, Ctx, RunningSession};
use crate::cli::PeekArgs;
use crate::zellij_cmd::ZellijEnv;

/// The session's whole screen as text, read over one ssh. The error names
/// the session, the node and the first line ssh or zellij had to say.
pub fn dump_screen(ctx: &Ctx, session: &RunningSession) -> Result<String> {
    let env = ZellijEnv::new(&ctx.cfg, session.job_id)?;
    let remote = env
        .remote_argv(["action", "dump-screen", "-p", "terminal_0", "--full"])
        .join(" ");
    let out = ssh_batch(&session.node, 10, &remote).output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "could not read the screen of session {} on {}: {}",
            session.job_id,
            session.node,
            first_stderr_line(&out)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The first non-blank stderr line, or the exit status when there is none.
pub fn first_stderr_line(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("ssh exited {}", out.status))
}

pub fn run(args: PeekArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let Some(session) = ctx.resolve_running(&args.target)? else {
        return Ok(1);
    };
    let screen = match dump_screen(&ctx, &session) {
        Ok(screen) => screen,
        Err(e) => {
            eprint_error(&ctx.palette(2), &format!("{e:#}"));
            return Ok(1);
        }
    };
    for line in tail_lines(&screen, args.lines) {
        println!("{line}");
    }
    Ok(0)
}

/// The last `n` lines of `screen` after trailing blank lines are dropped;
/// blank lines inside the window are kept (they are part of the output).
pub fn tail_lines(screen: &str, n: usize) -> Vec<&str> {
    let mut lines: Vec<&str> = screen.lines().map(|l| l.trim_end()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let start = lines.len().saturating_sub(n);
    lines.split_off(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_interior_blanks_and_trims_trailing() {
        let screen = "a\nb\n\nc\n\n\n   \n";
        assert_eq!(tail_lines(screen, 100), vec!["a", "b", "", "c"]);
        assert_eq!(tail_lines(screen, 2), vec!["", "c"]);
        assert_eq!(tail_lines(screen, 0), Vec::<&str>::new());
        assert!(tail_lines("\n\n", 5).is_empty());
    }
}
