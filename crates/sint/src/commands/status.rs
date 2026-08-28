//! `sinteractive status [TARGET] [--json]` and `refresh` — one session's
//! status. Ports `show_status` (script lines 1012-1139).
//!
//! `refresh` touches the session's `.poke` first so the in-session loop
//! re-checks its deadline and rewrites the cached state file; the live query
//! below already reports the truth, the poke is what makes the cache agree
//! within a tick. Fire-and-forget, as in 0.x.

use anyhow::Result;
use sint_core::notices;
use sint_core::session::SessionInfo;
use sint_core::time::format_short_duration;

use super::common::{eprint_error, print_json, resources_line, Ctx};
use crate::cli::TargetArgs;

/// `refresh` = true pokes the session's cache before reporting.
pub fn run(args: TargetArgs, refresh: bool) -> Result<i32> {
    let ctx = Ctx::new();

    if args.target.is_none() && ctx.cfg.job_id.is_none() {
        let p = ctx.palette(2);
        eprintln!(
            "{}{}Error:{}{} status/refresh requires a JOBID or NAME outside a session.{}",
            p.err, p.bold, p.reset, p.err, p.reset
        );
        eprintln!(
            "{}Run 'sinteractive list' to see available sessions.{}",
            p.dim, p.reset
        );
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

    let p = ctx.palette(1);
    // Green only for RUNNING; PENDING and every terminal state are things
    // you would want to notice, so they share the warning colour.
    let state_c = if info.state == "RUNNING" {
        &p.ok
    } else {
        &p.warn
    };
    let mut title = format!("{}Session{} {}{}{}", p.bold, p.reset, p.id, job_id, p.reset);
    if let Some(name) = &info.name {
        title.push_str(&format!(" {}({name}){}", p.bold, p.reset));
    }
    let on_node = match &info.node {
        Some(node) => format!(" {}on{} {}{node}{}", p.dim, p.reset, p.id, p.reset),
        None => String::new(),
    };
    println!("{title}: {state_c}{}{}{on_node}", info.state, p.reset);
    println!(
        "  {}{:<11}{} {}",
        p.key, "Partition:", p.reset, row.partition
    );
    println!(
        "  {}{:<11}{} {}",
        p.key,
        "Resources:",
        p.reset,
        resources_line(&row)
    );
    println!(
        "  {}{:<11}{} {} {}(limit {}){}",
        p.key, "Elapsed:", p.reset, row.elapsed, p.dim, row.time_limit, p.reset
    );
    if let Some(remaining) = info.remaining_seconds {
        // The one number worth reading at a glance, so it is coloured by how
        // much of it is left rather than left the same shade all session.
        let rem_c = if remaining < 900 {
            &p.err
        } else if remaining < 3600 {
            &p.warn
        } else {
            &p.ok
        };
        println!(
            "  {}{:<11}{} {rem_c}{}{}",
            p.key,
            "Remaining:",
            p.reset,
            format_short_duration(remaining),
            p.reset
        );
    }
    // The session's active notices — the full text behind the "⚠ N notices"
    // indicator on its status line, readable without attaching. Absent
    // file, nothing to say.
    for n in notices::read(&ctx.state, job_id) {
        if n.text.is_empty() {
            continue;
        }
        let n_c = if n.is_severe() {
            format!("{}{}", p.err, p.bold)
        } else {
            p.warn.clone()
        };
        println!(
            "  {}{:<11}{} {n_c}{}{}",
            p.key, "Notice:", p.reset, n.text, p.reset
        );
    }
    Ok(0)
}
