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
use sint_core::session::{comment_for, SessionInfo};

use super::common::{print_json, print_status_human, Ctx};
use super::launch::{bring_up, launch, Launched};
use crate::cli::{EnsureArgs, LaunchArgs};

/// The RUNNING or PENDING session named `name`, if any.
pub fn existing_session(ctx: &Ctx, name: &str) -> Result<Option<u64>> {
    let wanted = comment_for(Some(name));
    Ok(ctx
        .sessions()?
        .into_iter()
        .find(|r| r.comment == wanted)
        .map(|r| r.job_id))
}

/// What `ensure` found or made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ensured {
    /// The session, with `created` set either way.
    Session(Box<SessionInfo>),
    /// Slurm lost the job between finding (or making) it and describing it.
    NotFound(u64),
    /// The launch failed; the reason went to stderr, this is its exit code.
    Failed(i32),
}

/// Get-or-create without touching stdout: the existing session named
/// `name`, else one launched detached from `largs` under that name.
pub fn ensure_data(ctx: &Ctx, name: &str, mut largs: LaunchArgs) -> Result<Ensured> {
    let (job_id, created) = match existing_session(ctx, name)? {
        Some(job_id) => (job_id, false),
        None => {
            largs.name = Some(name.to_string());
            largs.detach = true;
            match bring_up(ctx, &largs)? {
                Launched::Ready { job_id, .. } => (job_id, true),
                Launched::Failed(code) => return Ok(Ensured::Failed(code)),
            }
        }
    };
    Ok(match ctx.session_info(job_id)? {
        Some(mut info) => {
            info.created = Some(created);
            Ensured::Session(Box::new(info))
        }
        None => Ensured::NotFound(job_id),
    })
}

pub fn run(args: EnsureArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let EnsureArgs {
        name,
        launch: mut largs,
    } = args;
    let p = ctx.palette(2);

    if let Some(job_id) = existing_session(&ctx, &name)? {
        if !largs.json {
            eprintln!(
                "{}✓{} {}Reusing existing session{} {}{name}{}{}.{}",
                p.ok, p.reset, p.dim, p.reset, p.id, p.reset, p.dim, p.reset
            );
        }
        let Some(mut info) = ctx.session_info(job_id)? else {
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
