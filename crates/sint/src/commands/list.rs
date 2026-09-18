//! `sinteractive list [--full] [--json]` — the user's running sessions.
//! Ports `list_sessions` (script lines 921-1010).
//!
//! Only RUNNING sessions are listed, as in 0.x. The JSON rows share the
//! `status --json` shape and additionally carry `cwd`. `--full` adds node,
//! partition and elapsed/limit to the human table; the default keeps to
//! what fits at a glance: job id, name, time remaining, cwd.

use std::thread;

use anyhow::Result;
use serde::Serialize;
use sint_core::color::Palette;
use sint_core::metrics::pane;
use sint_core::session::SessionInfo;
use sint_core::slurm::squeue::JobRow;
use sint_core::time::format_short_duration;

use super::common::{current_exe, print_json, remaining_colour, ssh_batch, tilde, Ctx};
use crate::cli::ListArgs;
use crate::zellij_cmd::shell_quote;

/// One `list --json` row: the status object plus `cwd`, always present
/// (null when it could not be found) and always the last key.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ListRow {
    #[serde(flatten)]
    pub info: SessionInfo,
    pub cwd: Option<String>,
}

/// `cwd` for every row, fetched over ssh in parallel — one `sinteractive
/// __pane-cwd JOBID` per node, backgrounded and joined the way 0.x
/// backgrounded one `tmux display-message` per session and `wait`ed (script
/// line 905). `None` on any failure — unreachable node, no cgroup yet, no
/// matching shell — so `list` never blocks or errors on a session whose cwd
/// can't be found.
fn fetch_cwds(rows: &[JobRow]) -> Vec<Option<String>> {
    let Ok(exe) = current_exe() else {
        return vec![None; rows.len()];
    };
    let exe = shell_quote(&exe.to_string_lossy());
    let handles: Vec<_> = rows
        .iter()
        .map(|row| {
            let node = row.node.clone();
            let job_id = row.job_id;
            let exe = exe.clone();
            thread::spawn(move || {
                let remote = format!("{exe} __pane-cwd {job_id}");
                let out = ssh_batch(&node, 3, &remote).output().ok()?;
                let cwd = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (!cwd.is_empty()).then_some(cwd)
            })
        })
        .collect();
    handles
        .into_iter()
        .map(|h| h.join().unwrap_or(None))
        .collect()
}

/// `sinteractive __pane-cwd JOBID` — runs on the node over ssh: the pane's
/// live working directory, `~`-collapsed, printed bare with a trailing
/// newline. Prints nothing when it cannot be found (a finished session, no
/// cgroup yet, no matching shell); [`fetch_cwds`] reads an empty line as
/// "unknown", same as an ssh failure.
pub fn run_pane_cwd(job_id: u64) -> Result<i32> {
    if let Some(cwd) = pane::session_cwd(job_id) {
        println!("{}", tilde(&cwd));
    }
    Ok(0)
}

/// The `list --json` rows: the user's RUNNING sessions, in squeue order.
pub fn list_data(ctx: &Ctx) -> Result<Vec<ListRow>> {
    let now = sint_core::now_epoch();
    let rows = ctx.running_sessions()?;
    let cwds = fetch_cwds(&rows);
    Ok(rows
        .iter()
        .zip(cwds)
        .map(|(row, cwd)| ListRow {
            info: SessionInfo::from_row(row, now),
            cwd,
        })
        .collect())
}

/// The padded, coloured REMAINING cell: [`remaining_colour`] applied
/// outside the padding, so the escape it wraps never counts toward the
/// column width.
fn remaining_cell(remaining: Option<i64>, width: usize, p: &Palette) -> String {
    match remaining {
        Some(r) => format!(
            "{}{:<width$}{}",
            remaining_colour(r, p),
            format_short_duration(r),
            p.reset,
            width = width
        ),
        None => format!("{}{:<width$}{}", p.dim, "-", p.reset, width = width),
    }
}

pub fn run(args: ListArgs) -> Result<i32> {
    let ctx = Ctx::new();
    if args.json {
        print_json(&list_data(&ctx)?)?;
        return Ok(0);
    }

    let rows = ctx.running_sessions()?;
    if rows.is_empty() {
        let p = ctx.palette(1);
        println!("{}No running sinteractive sessions.{}", p.dim, p.reset);
        println!("Start one with {}sinteractive{}.", p.key, p.reset);
        return Ok(0);
    }
    let cwds = fetch_cwds(&rows);

    // Colour goes outside every padded field, never inside it: an escape
    // counted as width would shift every column to its right.
    let p = ctx.palette(1);
    if args.full {
        println!(
            "{}{:<10}  {:<20}  {:<14}  {:<12}  {:<20}  {:<10}  CWD{}",
            p.dim, "JOBID", "NAME", "NODE", "PARTITION", "ELAPSED/LIMIT", "REMAINING", p.reset
        );
    } else {
        println!(
            "{}{:<10}  {:<20}  {:<10}  CWD{}",
            p.dim, "JOBID", "NAME", "REMAINING", p.reset
        );
    }
    for (row, cwd) in rows.iter().zip(&cwds) {
        let info = SessionInfo::from_row(row, sint_core::now_epoch());
        let name = info.name.as_deref().unwrap_or("-");
        let cwd = cwd.as_deref().unwrap_or("-");
        let remaining = remaining_cell(info.remaining_seconds, 10, &p);
        if args.full {
            println!(
                "{}{:<10}{}  {}{:<20}{}  {}{:<14}{}  {:<12}  {:<20}  {remaining}  {}{}{}",
                p.id,
                row.job_id,
                p.reset,
                p.bold,
                name,
                p.reset,
                p.id,
                row.node,
                p.reset,
                row.partition,
                format!("{}/{}", row.elapsed, row.time_limit),
                p.dim,
                cwd,
                p.reset
            );
        } else {
            println!(
                "{}{:<10}{}  {}{:<20}{}  {remaining}  {}{}{}",
                p.id, row.job_id, p.reset, p.bold, name, p.reset, p.dim, cwd, p.reset
            );
        }
    }

    println!();
    println!(
        "{}{:<10}{} sinteractive attach JOBID|NAME",
        p.key, "Reattach:", p.reset
    );
    println!(
        "{}{:<10}{} sinteractive cancel JOBID|NAME",
        p.key, "Cancel:", p.reset
    );
    Ok(0)
}
