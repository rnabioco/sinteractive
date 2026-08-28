//! `sinteractive list [--json]` — the user's running sessions. Ports
//! `list_sessions` (script lines 921-1010).
//!
//! Only RUNNING sessions are listed, as in 0.x. The JSON rows share the
//! `status --json` shape and additionally carry `cwd`.

use anyhow::Result;
use serde::Serialize;
use sint_core::session::SessionInfo;

use super::common::{print_json, Ctx};
use crate::cli::JsonFlag;

/// One `list --json` row: the status object plus `cwd`, which is always
/// present (null until phase 2 asks the node) and always the last key.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ListRow {
    #[serde(flatten)]
    pub info: SessionInfo,
    pub cwd: Option<String>,
}

/// The `list --json` rows: the user's RUNNING sessions, in squeue order.
pub fn list_data(ctx: &Ctx) -> Result<Vec<ListRow>> {
    let now = sint_core::now_epoch();
    Ok(ctx
        .running_sessions()?
        .iter()
        .map(|row| ListRow {
            info: SessionInfo::from_row(row, now),
            // TODO(phase-2): cwd via zellij list-panes on the node (0.x asked
            // tmux over ssh, script line 905). Until then every row reports
            // null; the key is part of the contract so it is always present.
            cwd: None,
        })
        .collect())
}

pub fn run(args: JsonFlag) -> Result<i32> {
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
    let cwd: Option<String> = None;

    // Colour goes outside every padded field, never inside it: an escape
    // counted as width would shift every column to its right.
    let p = ctx.palette(1);
    println!(
        "{}{:<10}  {:<20}  {:<14}  {:<12}  {:<10}  {:<10}  CWD{}",
        p.dim, "JOBID", "NAME", "NODE", "PARTITION", "ELAPSED", "TIMELIMIT", p.reset
    );
    for row in &rows {
        let info = SessionInfo::from_row(row, 0);
        let name = info.name.as_deref().unwrap_or("-");
        let cwd = cwd.as_deref().unwrap_or("-");
        println!(
            "{}{:<10}{}  {}{:<20}{}  {}{:<14}{}  {:<12}  {:<10}  {:<10}  {}{}{}",
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
            row.elapsed,
            row.time_limit,
            p.dim,
            cwd,
            p.reset
        );
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
