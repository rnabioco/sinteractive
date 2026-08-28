//! `sinteractive list [--json]` — the user's running sessions. Ports
//! `list_sessions` (script lines 921-1010).
//!
//! Only RUNNING sessions are listed, as in 0.x. The JSON rows share the
//! `status --json` shape and additionally carry `cwd`.

use anyhow::Result;
use serde_json::Value;
use sint_core::session::{sessions_only, SessionInfo};

use super::common::{print_json, Ctx};
use crate::cli::JsonFlag;

pub fn run(args: JsonFlag) -> Result<i32> {
    let ctx = Ctx::new();
    let rows = ctx.slurm.my_jobs(&["RUNNING"])?;
    let rows = sessions_only(&rows);

    if rows.is_empty() {
        if args.json {
            println!("[]");
        } else {
            let p = ctx.palette(1);
            println!("{}No running sinteractive sessions.{}", p.dim, p.reset);
            println!("Start one with {}sinteractive{}.", p.key, p.reset);
        }
        return Ok(0);
    }

    // TODO(phase-2): cwd via zellij list-panes on the node (0.x asked tmux
    // over ssh, script line 905). Until then every row reports null; the
    // key is part of the contract so it is always present.
    let cwd: Option<String> = None;

    if args.json {
        let now = sint_core::now_epoch();
        let out: Vec<Value> = rows
            .iter()
            .map(|row| {
                let mut v = serde_json::to_value(SessionInfo::from_row(row, now))?;
                if let Value::Object(m) = &mut v {
                    m.insert("cwd".into(), Value::from(cwd.clone()));
                }
                Ok(v)
            })
            .collect::<Result<_>>()?;
        print_json(&out)?;
        return Ok(0);
    }

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
