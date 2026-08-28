//! `sinteractive hook session-start|prompt` — Claude Code hook entry points.
//!
//! Native replacements for `claude/hooks/sinteractive-session-context.sh`
//! and `sinteractive-walltime-guard.sh`. Claude Code adds a `SessionStart`
//! or `UserPromptSubmit` hook's plain stdout to the agent's context, so the
//! output is prose, not JSON.
//!
//! Both **always exit 0**: a nonzero hook puts an error notice in the
//! transcript, and every reason these can bail — not in a session, no state
//! file, scheduler unreachable — is a normal condition, not a failure.
//!
//! `prompt` is silent above the threshold (`SINTERACTIVE_AGENT_WARN`,
//! default 1800 s), which is the common case: no output, no scheduler
//! traffic. It prefers the session's cached state file, aged exactly
//! (`remaining_seconds - (now - updated_epoch)`), and only asks Slurm when
//! the cache is missing or older than two minutes. `UserPromptSubmit` rather
//! than `PreToolUse`: once per turn is the right cadence for "can this
//! finish?", and it keeps the check off the path of every Bash call.

use anyhow::Result;
use sint_core::now_epoch;
use sint_core::session::SessionInfo;

use super::common::Ctx;
use crate::cli::{HookArgs, HookEvent};

pub fn run(args: HookArgs) -> Result<i32> {
    match args.event {
        HookEvent::SessionStart => session_start(),
        HookEvent::Prompt => prompt(),
    }
}

/// The agent briefing, when inside a session; silence otherwise.
fn session_start() -> Result<i32> {
    let ctx = Ctx::new();
    if ctx.cfg.job_id.is_none() {
        return Ok(0);
    }
    // agent-context prints its own errors on stderr and exits 1 when the
    // job is gone; the hook still exits 0.
    let _ = super::agent_context::run();
    Ok(0)
}

/// `(remaining_seconds, end_epoch)` for the current session, or `None`.
fn budget(ctx: &Ctx, job_id: u64, now: i64) -> Option<(i64, Option<i64>)> {
    if let Some(state) = ctx.state.read_state(job_id) {
        if let Some(rem) = state.aged_remaining(now) {
            return Some((rem, state.end_epoch));
        }
    }
    let row = ctx.slurm.job(job_id).ok().flatten()?;
    let info = SessionInfo::from_row(&row, now);
    let rem = info.remaining_seconds?;
    Some((rem, info.end_epoch))
}

/// Text of the warning, or `None` when there is nothing to say.
pub fn walltime_warning(
    job_id: u64,
    remaining: i64,
    end_epoch: Option<i64>,
    warn_at: i64,
) -> Option<String> {
    let remaining = remaining.max(0);
    if remaining > warn_at {
        return None;
    }
    let left = if remaining >= 3600 {
        format!("{}h {}m", remaining / 3600, (remaining % 3600) / 60)
    } else if remaining >= 60 {
        format!("{}m", remaining / 60)
    } else {
        format!("{remaining}s")
    };
    let ends = end_epoch
        .map(|e| format!(" (ends {})", local_hhmm(e)))
        .unwrap_or_default();
    Some(format!(
        "Walltime warning: this sinteractive session (job {job_id}) has\n\
         {left} left{ends}. It self-terminates ~10s before the limit, which ends this\n\
         shell and anything attached to it, including an srun you are streaming from.\n\
         \n\
         Do not start work that cannot finish in that window. Long work belongs in its\n\
         own allocation with its own -t (salloc --no-shell + srun --overlap), which\n\
         survives independently — or ask the user for a fresh session.\n"
    ))
}

/// `HH:MM` local time for an epoch.
fn local_hhmm(epoch: i64) -> String {
    let s = sint_core::notices::format_local_datetime(epoch);
    // `%a %b %-d %H:%M` — the time is the last word.
    s.rsplit(' ').next().unwrap_or("").to_string()
}

fn prompt() -> Result<i32> {
    let ctx = Ctx::new();
    let Some(job_id) = ctx.cfg.job_id else {
        return Ok(0);
    };
    let now = now_epoch();
    let Some((remaining, end_epoch)) = budget(&ctx, job_id, now) else {
        return Ok(0);
    };
    if let Some(text) = walltime_warning(job_id, remaining, end_epoch, ctx.cfg.agent_warn) {
        print!("{text}");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_above_threshold() {
        assert!(walltime_warning(1, 1801, None, 1800).is_none());
        assert!(walltime_warning(1, 1800, None, 1800).is_some());
    }

    #[test]
    fn wording_and_units() {
        let t = walltime_warning(147845, 3725, None, 7200).unwrap();
        assert!(t.starts_with(
            "Walltime warning: this sinteractive session (job 147845) has\n1h 2m left."
        ));
        let t = walltime_warning(1, 125, None, 1800).unwrap();
        assert!(t.contains("\n2m left."));
        let t = walltime_warning(1, 59, None, 1800).unwrap();
        assert!(t.contains("\n59s left."));
        let t = walltime_warning(1, -5, None, 1800).unwrap();
        assert!(t.contains("\n0s left."));
        assert!(t.contains("salloc --no-shell + srun --overlap"));
    }

    #[test]
    fn ends_clause_is_hhmm() {
        let t = walltime_warning(1, 600, Some(1788422100), 1800).unwrap();
        let ends = t.lines().nth(1).unwrap();
        assert!(ends.contains("left (ends "), "{ends}");
        let i = ends.find("(ends ").unwrap() + "(ends ".len();
        let hhmm = &ends[i..i + 5];
        assert_eq!(&hhmm[2..3], ":", "{hhmm}");
        assert_eq!(&ends[i + 5..i + 7], ").");
    }
}
