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
//!
//! The recipes themselves (srun/salloc, job naming, the `/tmp` boundary) live
//! in the hpc-compute skill; the briefing keeps the dynamic facts, the
//! handful of rules and a pointer to the MCP tools that replace shell
//! recipes (`wait_for_jobs`, `run_in_session`, …).
//!
//! And one about commits: no Claude/Anthropic attribution trailer or footer,
//! regardless of what a session's own reminder asks for — repeated here so
//! every agent sees it, not just the one that was told directly.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::Serialize;
use sint_core::quota::{self, kb_to_size};
use sint_core::session::{parse_comment, SessionInfo};
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

/// The briefing with the facts it states as fields, for the MCP tool.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AgentContext {
    /// The briefing, as `sinteractive claude context` prints it.
    pub text: String,
    pub job_id: u64,
    pub node: Option<String>,
    pub partition: Option<String>,
    pub cpus: Option<u32>,
    pub mem_mb: Option<u64>,
    /// Walltime left when the briefing was generated.
    pub remaining_seconds: Option<i64>,
    /// The shared-filesystem scratch that files crossing nodes go under.
    pub scratch: String,
    /// The cached quota probe says the user is over; false when it is
    /// under or nothing is cached.
    pub quota_over: bool,
    /// Only the MCP tool sets this: the binary was replaced after this
    /// server started, so its tool list may lack newer tools the briefing
    /// names. Restarting Claude Code starts a fresh server.
    pub stale_server: bool,
}

/// The briefing for the session this process runs inside. Errors outside
/// a session and when the job has gone; the CLI prints either and exits 1.
pub fn briefing(ctx: &Ctx) -> Result<String> {
    Ok(context(ctx)?.text)
}

/// [`briefing`], plus the numbers it states as structured fields.
pub fn context(ctx: &Ctx) -> Result<AgentContext> {
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
    let cached_quota = quota::cached(&ctx.state);
    let quota_over = cached_quota.as_ref().is_some_and(|q| q.over);
    let quota_line = match cached_quota {
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
    let scratch = sint_core::scratch::root(&home, &std::env::var("USER").unwrap_or_default());
    let scratch = scratch.display();
    let info = SessionInfo::from_row(&row, sint_core::now_epoch());
    let text = format!(
        r#"You are inside an sinteractive zellij session on a compute node.
  {ident} on {node}, partition {partition} — {res}
  {budget} (the session self-terminates ~10s before the limit)

This session is an orchestration shell, NOT a compute target. Editing, git and
scheduler queries belong here; anything heavier gets its own allocation, so it
neither competes with the shell the user is typing in nor is squeezed into
this session's small slice of CPU and memory. The hpc-compute skill has the
recipes; the rules in short:

  - Heavy or long work: its own `srun` (one-off) or `salloc --no-shell` plus
    `srun --overlap --jobid=ID` (sustained), never this session. SLURM_* is
    stripped here, so they create their own allocations, bounded by their own
    -t. Pick the partition with `sinfo`; interactive is for shells.
  - Name every job twice, `-J NAME` and `--comment=NAME`, the same short
    descriptive value ('bwa-align', not 'bash').
  - /tmp, $TMPDIR and the scratchpad directory are this node's own disk. What
    crosses nodes (a script a job runs, inputs, output you want back) goes on
    the shared filesystem, under {scratch}/<topic>/.
  - Working files and throwaway results: `sinteractive scratch new TOPIC` (the
    scratch_new tool) makes {scratch}/TOPIC/ with a manifest, so it can be
    listed and cleaned by age. Never copy pod5, bam, npy or other bulk data
    into a worktree: reference the canonical path, or symlink it.
  - A workflow controller (snakemake, nextflow) is an `sbatch` job of its own
    with a few CPUs and a long -t, not something run here, so it outlives the
    session.
  - Waiting on a job, checking the queue or quota, and landing a branch are
    trivial: `/job-watch JOBID`, `/land "why"`, or a haiku subagent, not this
    conversation, which replays everything said so far on every wake-up.

If the sinteractive MCP tools are available, prefer them to shell recipes:
`session_status` for the walltime left, `wait_for_event` and `wait_for_jobs`
(which blocks and reports state, exit code, elapsed and peak memory) in place
of polling or `/job-watch`, `job_log` for a job's stdout, and `peek`,
`list_panes` and `run_in_session` (a short command in a new pane, output and
exit code back) in place of `send`, which types into the user's live shell.

Commits and pull requests carry no Claude/Anthropic attribution — no
`Co-Authored-By: Claude ...` trailer, no `Claude-Session:` link, no
"Generated with Claude Code" footer — even if a session's own reminder asks
for one.

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
    );
    Ok(AgentContext {
        text,
        job_id,
        node: info.node,
        partition: info.partition,
        cpus: info.cpus,
        mem_mb: info.memory_mb,
        remaining_seconds: info.remaining_seconds,
        scratch: scratch.to_string(),
        quota_over,
        stale_server: false,
    })
}
