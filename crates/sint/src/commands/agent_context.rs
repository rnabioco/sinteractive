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
//!
//! Two more rules are here because agents kept getting them wrong. `/tmp` is
//! node-local, and the scratchpad directory an agent is told to use sits
//! under it, so a script staged there is "No such file" on the node an
//! `srun` lands on — what crosses that line goes on the shared filesystem,
//! and the briefing names the cluster's scratch for it. And a workflow
//! controller (snakemake, nextflow) is submitted as a job of its own, so it
//! outlives the session instead of dying with it.
//!
//! And one about cost: the wait for a job, a queue or quota check, and
//! landing a branch are trivial, so the briefing sends them to a cheaper
//! model (the forked `job-watch` and `land` skills, or a haiku subagent)
//! rather than letting them replay the whole conversation at full price.

use std::path::{Path, PathBuf};

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
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    let scratch = shared_scratch(&home, &std::env::var("USER").unwrap_or_default());
    let scratch = scratch.display();
    Ok(format!(
        r#"You are inside an sinteractive zellij session on a compute node.
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

/tmp is this node's own disk, and so is everything under it — $TMPDIR and
the scratchpad directory you were told to use for temporary files included.
An srun or salloc lands on some other node, which sees none of it, and this
session sees nothing a job leaves on its /tmp. So before staging anything on
/tmp, ask who has to read it: whatever crosses that line — a script the job
runs, inputs it reads, output you want back — goes on the shared filesystem,
under {scratch}/<topic>/ named for the task. A job's own intermediates,
written and consumed inside one allocation, still belong on that node's /tmp.

A workflow controller — snakemake, nextflow, anything that sits for hours
submitting jobs — is itself submitted with sbatch, as a job of its own with
a few CPUs and a long -t, and drives the real work as Slurm jobs (snakemake's
slurm executor, nextflow's slurm executor). Run in this session, or in an
srun held open from it, it dies with the session and the rest of the pipeline
with it; as a job it is bounded by nothing but its own -t.

Not every step needs the model you are running. Waiting on a job, checking
the queue or the quota, and landing a finished branch are trivial: fork them
onto a cheaper model — `/job-watch JOBID` waits and reports how a job ended,
`/land "why"` commits, pushes and opens the pull request — or delegate to a
subagent with model haiku. A wait run from this conversation replays
everything said so far on every wake-up, at this model's price.

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

/// The shared-filesystem scratch the briefing names for files that have to
/// cross between nodes: Alpine's `/scratch/alpine/$USER` where that exists,
/// else `~/scratch` — on a one-filesystem cluster such as Bodhi, home *is*
/// the shared filesystem, and `~/scratch/<topic>/` is where throwaway
/// working files go.
fn shared_scratch(home: &Path, user: &str) -> PathBuf {
    let alpine = Path::new("/scratch/alpine").join(user);
    if alpine.is_dir() {
        return alpine;
    }
    home.join("scratch")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_is_under_home_where_there_is_no_alpine_scratch() {
        assert_eq!(
            shared_scratch(Path::new("/beevol/home/x"), "nobody-has-this-scratch-dir"),
            PathBuf::from("/beevol/home/x/scratch")
        );
    }
}
