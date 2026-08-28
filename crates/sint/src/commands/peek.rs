//! `sinteractive peek TARGET [-n LINES]` — the last lines of a session's
//! shell pane, read over one ssh with the embedded zellij's `dump-screen`.
//! Replaces the 0.x `ssh NODE tmux capture-pane` recipe the hpc-compute
//! skill used to document.

use anyhow::Result;

use super::common::{eprint_error, ssh_batch, Ctx};
use crate::cli::PeekArgs;
use crate::zellij_cmd::ZellijEnv;

pub fn run(args: PeekArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let Some(session) = ctx.resolve_running(&args.target)? else {
        return Ok(1);
    };
    let env = ZellijEnv::new(&ctx.cfg, session.job_id)?;
    let remote = env
        .remote_argv(["action", "dump-screen", "-p", "terminal_0", "--full"])
        .join(" ");
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
            &ctx.palette(2),
            &format!(
                "could not read the screen of session {} on {}: {detail}",
                session.job_id, session.node
            ),
        );
        return Ok(1);
    }
    let screen = String::from_utf8_lossy(&out.stdout);
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
