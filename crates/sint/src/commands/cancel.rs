//! `sinteractive cancel TARGET [--json]` — scancel with name resolution.
//! Ports `cancel_session` (script lines 2182-2207). Works from inside a
//! session too: cancelling the session you are in ends it, like walltime
//! running out.

use anyhow::Result;
use sint_core::session::parse_comment;
use sint_core::slurm::SlurmError;

use super::common::{eprint_error, print_json, Ctx};
use crate::cli::CancelArgs;

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

    let id = job_id.to_string();
    if let Err(e) = ctx.slurm.run("scancel", &[&id]) {
        let detail = match &e {
            SlurmError::Failed { stderr, .. } if !stderr.is_empty() => stderr.clone(),
            SlurmError::Failed { .. } => "scancel failed".to_string(),
            other => other.to_string(),
        };
        eprint_error(
            &ctx.palette(2),
            &format!("could not cancel job {job_id}: {detail}"),
        );
        if args.json {
            print_json(&serde_json::json!({"job_id": job_id, "cancelled": false}))?;
        }
        return Ok(1);
    }

    if args.json {
        print_json(&serde_json::json!({"job_id": job_id, "cancelled": true}))?;
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
