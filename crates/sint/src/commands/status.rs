//! `sinteractive status [TARGET] [--json]` and `refresh` — one session's
//! status. Ports `show_status` (script lines 1012-1139).
//!
//! `refresh` touches the session's `.poke` first so the in-session loop
//! re-checks its deadline and rewrites the cached state file; the live query
//! below already reports the truth, the poke is what makes the cache agree
//! within a tick. Fire-and-forget, as in 0.x.

use anyhow::Result;
use sint_core::session::SessionInfo;

use super::common::{eprint_error, missing_target, print_json, print_status_human, Ctx};
use crate::cli::TargetArgs;

/// `refresh` = true pokes the session's cache before reporting.
pub fn run(args: TargetArgs, refresh: bool) -> Result<i32> {
    let ctx = Ctx::new();

    if missing_target(&ctx, args.target.as_deref(), "status/refresh") {
        return Ok(1);
    }
    let Some(job_id) = ctx.resolve_reporting(args.target.as_deref())? else {
        return Ok(1);
    };

    if refresh {
        let _ = ctx.state.poke(job_id);
    }

    let Some(row) = ctx.slurm.job(job_id)? else {
        if args.json {
            print_json(&SessionInfo::not_found(job_id))?;
        } else {
            eprint_error(
                &ctx.palette(2),
                &format!("job {job_id} not found (finished or cancelled)"),
            );
        }
        return Ok(1);
    };

    let info = SessionInfo::from_row(&row, sint_core::now_epoch());
    if args.json {
        print_json(&info)?;
        return Ok(0);
    }

    print_status_human(&ctx, &info);
    Ok(0)
}
