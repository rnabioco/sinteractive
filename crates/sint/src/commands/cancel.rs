//! `sinteractive cancel TARGET [--json]` — scancel with name resolution.
//! Ports `cancel_session` (script lines 2182-2207). Works from inside a
//! session too: cancelling the session you are in ends it, like walltime
//! running out.

use anyhow::Result;
use serde::Serialize;
use sint_core::session::parse_comment;
use sint_core::slurm::SlurmError;

use super::common::{eprint_error, print_json, Ctx};
use crate::cli::CancelArgs;

/// The `cancel --json` object. `detail` is scancel's complaint when it
/// refused, for the caller to report; it is not part of the JSON.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct CancelResult {
    pub job_id: u64,
    pub cancelled: bool,
    #[serde(skip)]
    pub detail: Option<String>,
}

/// `scancel JOBID`. A refusal is reported in the result rather than as an
/// error, so the JSON contract (`cancelled: false`) holds either way.
pub fn cancel_job(ctx: &Ctx, job_id: u64) -> CancelResult {
    let id = job_id.to_string();
    let detail = ctx.slurm.run("scancel", &[&id]).err().map(|e| match e {
        SlurmError::Failed { stderr, .. } if !stderr.is_empty() => stderr,
        SlurmError::Failed { .. } => "scancel failed".to_string(),
        other => other.to_string(),
    });
    CancelResult {
        job_id,
        cancelled: detail.is_none(),
        detail,
    }
}

pub fn run(args: CancelArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let Some(job_id) = ctx.resolve_reporting(Some(&args.target))? else {
        return Ok(1);
    };

    // Describe the session before it disappears from squeue.
    let (name, node) = match ctx.slurm.job(job_id)? {
        Some(row) => (parse_comment(&row.comment).flatten(), row.node),
        None => (None, String::new()),
    };

    let result = cancel_job(&ctx, job_id);
    if let Some(detail) = &result.detail {
        eprint_error(
            &ctx.palette(2),
            &format!("could not cancel job {job_id}: {detail}"),
        );
        if args.json {
            print_json(&result)?;
        }
        return Ok(1);
    }

    if args.json {
        print_json(&result)?;
        return Ok(0);
    }

    let p = ctx.palette(2);
    let mut desc = format!("session {job_id}");
    if let Some(name) = name {
        desc.push_str(&format!(" ({name})"));
    }
    let on_node = if node.is_empty() {
        String::new()
    } else {
        format!(" on {}{node}{}", p.id, p.reset)
    };
    eprintln!("{}✓{} Cancelled {desc}{on_node}.", p.ok, p.reset);
    Ok(0)
}
