//! `sinteractive __popup VIEW JOBID` — what the status bar's keybindings
//! open in a floating pane. `queue` (Ctrl+b q) is the live queue view;
//! `rename` (Ctrl+b $) prompts for a new session name; `help`, `notices`
//! and `monitor` are rendered inline by the plugin, so this only tells a
//! stray caller where they went.

use std::io::{BufRead, Write};

use anyhow::Result;
use sint_core::session::{comment_for, parse_comment, validate_name};

use super::common::{eprint_error, Ctx};
use super::queue;
use crate::cli::{PopupView, QueueArgs};

pub fn run(view: PopupView, job_id: Option<u64>) -> Result<i32> {
    let job_id = job_id
        .or_else(|| {
            std::env::var("SINTERACTIVE_JOB_ID")
                .ok()
                .and_then(|v| v.trim().parse().ok())
        })
        .ok_or_else(|| anyhow::anyhow!("__popup needs a JOBID or SINTERACTIVE_JOB_ID"))?;
    match view {
        PopupView::Queue => queue::run(QueueArgs {
            watch: true,
            ..QueueArgs::default()
        }),
        PopupView::Rename => rename(job_id),
        PopupView::Help | PopupView::Notices | PopupView::Monitor => {
            println!("{view:?} is handled by the status bar.");
            Ok(0)
        }
    }
}

/// Prompt for a name and store it in the job's Comment, which is where a
/// session's name lives (`sinteractive:NAME`); the node-side loop reads it
/// back on its next poll and the bar, state file and `status` follow. A
/// poke makes that next poll now.
fn rename(job_id: u64) -> Result<i32> {
    let ctx = Ctx::new();
    let p = ctx.palette(2);
    let current = ctx
        .slurm
        .job(job_id)?
        .and_then(|r| parse_comment(&r.comment).flatten());
    let hint = current.as_deref().unwrap_or("");
    eprint!("{}session name{} [{hint}]: ", p.key, p.reset);
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let name = line.trim();
    if name.is_empty() {
        eprintln!("{}unchanged{}", p.dim, p.reset);
        return Ok(0);
    }
    if let Err(e) = validate_name(name) {
        eprint_error(&p, &e);
        std::thread::sleep(std::time::Duration::from_secs(2));
        return Ok(1);
    }
    ctx.slurm.set_comment(job_id, &comment_for(Some(name)))?;
    let _ = ctx.state.poke(job_id);
    eprintln!("{}✓{} renamed to {}{name}{}", p.ok, p.reset, p.id, p.reset);
    std::thread::sleep(std::time::Duration::from_millis(800));
    Ok(0)
}
