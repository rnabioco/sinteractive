//! `sinteractive agent-context` — briefing for a coding agent running inside
//! a session (Claude Code and friends), printed on stdout; exits 1 with a
//! one-line note when run outside one. Ports `agent_context` (script lines
//! 1140-1236).
//!
//! The wording lives here rather than in the SessionStart hook so it is
//! versioned with the tool, and so a human can run the command to see
//! exactly what the agent was told.
//!
//! The load-bearing message is that the session is NOT a compute target: it
//! is usually a small allocation on the interactive partition, and work run
//! in it competes with the shell the user is typing in. That is also why the
//! resource numbers below are reported but never exported as environment
//! variables — a `SINTERACTIVE_CPUS` in the environment is an invitation to
//! run `make -j` here.

use anyhow::{anyhow, Result};
use sint_core::quota::{self, kb_to_size};
use sint_core::session::parse_comment;
use sint_core::time::{format_short_duration, slurm_timestamp_to_epoch};

use super::common::{eprint_error, resources_line, Ctx};

pub fn run() -> Result<i32> {
    let ctx = Ctx::new();
    match briefing(&ctx) {
        Ok(text) => {
            print!("{text}");
            Ok(0)
        }
        Err(e) => {
            eprint_error(&ctx.palette(2), &format!("{e:#}"));
            Ok(1)
        }
    }
}

/// The briefing for the session this process runs inside. Errors outside
/// a session and when the job has gone; the CLI prints either and exits 1.
pub fn briefing(ctx: &Ctx) -> Result<String> {
    let Some(job_id) = ctx.cfg.job_id else {
        return Err(anyhow!(
            "not inside an sinteractive session (SINTERACTIVE_JOB_ID unset)."
        ));
    };

    let Some(row) = ctx.slurm.job(job_id)? else {
        return Err(anyhow!("job {job_id} not found (finished or cancelled)"));
    };

    let name = parse_comment(&row.comment).flatten();
    let res = resources_line(&row);

    let budget = match slurm_timestamp_to_epoch(&row.end_time) {
        Some(end) => {
            let remaining = (end - sint_core::now_epoch()).max(0);
            format!("walltime {} remaining", format_short_duration(remaining))
        }
        None => "walltime unknown".to_string(),
    };

    let mut ident = format!("job {job_id}");
    if let Some(name) = &name {
        ident.push_str(&format!(" ({name})"));
    }

    // Only stated when it is true and already known. The briefing runs at
    // every session start, so it reads the cache rather than probing nine
    // daemons — and says nothing at all when there is no cache or the user
    // is under.
    let quota_line = match quota::cached(&ctx.state) {
        Some(q) if q.over => format!(
            "\nOVER STORAGE QUOTA: {} of {} used ({}%), over by {}.",
            kb_to_size(q.used_kb),
            kb_to_size(q.hard_kb),
            q.pct,
            kb_to_size(q.over_kb)
        ),
        _ => String::new(),
    };

    let node = &row.node;
    let partition = &row.partition;
    Ok(format!(
        r#"You are inside an sinteractive tmux session on a compute node.
  {ident} on {node}, partition {partition} — {res}
  {budget} (the session self-terminates ~10s before the limit)

This session is an orchestration shell, NOT a compute target. Editing, git and
scheduler queries belong here; anything heavier gets its own allocation, so it
neither competes with the shell the user is typing in nor is squeezed into
this session's small slice of CPU and memory.

  one-off     srun -p PART -c N --mem SIZE -t TIME -J NAME --comment=NAME -- CMD
  sustained   salloc --no-shell -p PART -c N --mem SIZE -t TIME -J NAME --comment=NAME
              srun --overlap --jobid=ID -- CMD                    # repeat as needed
              scancel ID                                          # when done

Both stream output and propagate the command's exit code. salloc announces
itself with "salloc: Granted job allocation ID" on stderr, not stdout, so
capture it through 2>&1.

Name every job in both fields — -J NAME and --comment=NAME, the same short
descriptive value ('bwa-align', not 'bash') — so a shared queue says what is
running and why; `squeue --me -o "%.10i %.20j %.20P %.10M %k"` shows the name
in %j and the comment in %k. Both belong to the allocation, so naming the
salloc covers its srun --overlap steps.

Pick PART with `sinfo`; the interactive partition has few nodes, caps
walltime and carries everyone's shells, so it is not the right target for
work — send it to a compute partition instead. SLURM_* is stripped from this
session, so srun and salloc create their own allocations rather than steps of
this job — and work in them is bounded by their own -t, not by the budget above.

Re-check this session with `sinteractive status --json` before long work;
the number above was read when this briefing was generated, and a walltime can
be changed underneath you.
{quota_line}
Storage quota, while exceeded, is a red notice behind the "N notices" counter
on the status line (shown in full by `sinteractive status`).
Check it with `sinteractive quota --check`, and run that again after
deleting anything on the user's behalf — it refreshes every open session, so
the warning clears immediately rather than up to ten minutes later.
"#
    ))
}
