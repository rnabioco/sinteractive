//! `sinteractive ensure NAME` — get-or-create (script lines 493-502, 785).
//!
//! An existing RUNNING or PENDING session by that name is reported with
//! `created:false` and exit 0 — deliberately quiet about a miss, since not
//! finding one is the create path, not an error. A PENDING match counts as
//! existing and is reported with its real state rather than waited on, so
//! the caller decides whether to poll. Otherwise the session is launched
//! detached under that name and reported with `created:true`. The launch
//! re-checks for duplicates, so two concurrent calls can still both create;
//! that race is not worth a lock.

use anyhow::Result;
use sint_core::now_epoch;
use sint_core::session::{comment_for, SessionInfo};

use super::common::{print_json, Ctx};
use super::launch::{launch, print_status_human};
use crate::cli::EnsureArgs;

pub fn run(args: EnsureArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let EnsureArgs {
        name,
        launch: mut largs,
    } = args;
    let p = ctx.palette(2);

    let wanted = comment_for(Some(&name));
    let existing = ctx
        .sessions()?
        .into_iter()
        .find(|r| r.comment == wanted)
        .map(|r| r.job_id);
    if let Some(job_id) = existing {
        if !largs.json {
            eprintln!(
                "{}✓{} {}Reusing existing session{} {}{name}{}{}.{}",
                p.ok, p.reset, p.dim, p.reset, p.id, p.reset, p.dim, p.reset
            );
        }
        let Some(row) = ctx.slurm.job(job_id)? else {
            if largs.json {
                print_json(&SessionInfo::not_found(job_id))?;
            } else {
                eprintln!(
                    "{}sinteractive:{} job {job_id} not found (finished or cancelled){}",
                    p.err, p.reset, p.reset
                );
            }
            return Ok(1);
        };
        let mut info = SessionInfo::from_row(&row, now_epoch());
        if largs.json {
            info.created = Some(false);
            print_json(&info)?;
        } else {
            print_status_human(&ctx, &info);
        }
        return Ok(0);
    }

    largs.name = Some(name);
    largs.detach = true;
    launch(&ctx, largs, Some(true))
}
