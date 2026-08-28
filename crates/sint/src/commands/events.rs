//! `sinteractive events [TARGET] [--follow] [--since EPOCH]` — a session's
//! event log as NDJSON, one object per line (see [`sint_core::events`]).
//!
//! The log lives in `<cache>/<jobid>.events.ndjson`, appended by the
//! session's own loop, so it reads from the login node without ssh.
//! `--follow` polls once a second and stops when the session is over: an
//! `ended` event has been printed, or the session's state file
//! (`<jobid>.json`) has disappeared after having been seen. Before the
//! session's first tick neither file exists yet; `--follow` waits.

use std::time::Duration;

use anyhow::Result;
use sint_core::events::{self, Event};
use sint_core::state::StateDir;

use super::common::Ctx;
use crate::cli::EventsArgs;

/// How often `--follow` re-reads the log.
pub const FOLLOW_POLL: Duration = Duration::from_secs(1);

/// The events after `since` (all of them when `None`).
fn read(state: &StateDir, job_id: u64, since: Option<i64>) -> Result<Vec<Event>> {
    Ok(match since {
        Some(ts) => events::read_since(state, job_id, ts)?,
        None => events::read_all(state, job_id)?,
    })
}

pub fn run(args: EventsArgs) -> Result<i32> {
    let ctx = Ctx::new();
    if args.target.is_none() && ctx.cfg.job_id.is_none() {
        let p = ctx.palette(2);
        eprintln!(
            "{}{}Error:{}{} events requires a JOBID or NAME outside a session.{}",
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

    let mut printed = 0usize;
    let mut state_seen = ctx.state.state_file(job_id).exists();
    loop {
        let evs = read(&ctx.state, job_id, args.since)?;
        let mut ended = false;
        for ev in evs.iter().skip(printed) {
            println!("{}", ev.to_line());
            ended |= ev.kind == "ended";
        }
        printed = evs.len();
        if !args.follow || ended {
            return Ok(0);
        }
        let state_now = ctx.state.state_file(job_id).exists();
        if state_seen && !state_now {
            // The session tore its files down between two polls: whatever
            // was appended last has been printed above.
            return Ok(0);
        }
        state_seen |= state_now;
        std::thread::sleep(FOLLOW_POLL);
    }
}
