//! `sinteractive quota [--check] [--json]` — storage quota (Bodhi).
//!
//! `quota` reports the cached probe; `quota --check` probes now, rewrites
//! the cache, pokes every live session so the fresh answer reaches their
//! notices on the next tick, and reports. Ports `check_quota_cmd` (script
//! lines 1437-1476). Exits 0 whether or not the user is over — being over
//! quota is a fact to report, not an error in the check.

use std::fs;

use anyhow::Result;
use sint_core::quota::{self, kb_to_size, QuotaSnapshot};

use super::common::{eprint_error, Ctx};
use crate::cli::QuotaArgs;

/// The snapshot to report: a fresh probe when `check` (cached, and every
/// live session poked so its notices catch up on the next tick), else the
/// cache. `None` when neither is available.
pub fn quota_data(ctx: &Ctx, check: bool) -> Result<Option<QuotaSnapshot>> {
    if !check {
        return Ok(quota::cached(&ctx.state));
    }
    let (user, uid) = quota::current_user();
    let Ok(snap) = quota::probe(&ctx.cfg, &user, uid) else {
        return Ok(None);
    };
    quota::write_cache(&ctx.state, &snap)?;
    // Fire-and-forget, as in 0.x.
    let _ = ctx.state.poke_all();
    Ok(Some(snap))
}

pub fn run(args: QuotaArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let snap = quota_data(&ctx, args.check)?;

    let Some(snap) = snap else {
        if args.json {
            println!(r#"{{"error":"quota unavailable"}}"#);
        } else {
            let p = ctx.palette(2);
            eprint_error(&p, "could not read quota.");
            eprintln!();
            if args.check {
                eprintln!(
                    "{}Needs the quota file ({}){}",
                    p.dim,
                    ctx.cfg.quota_file.display(),
                    p.reset
                );
                eprintln!(
                    "{}and at least one reachable quota daemon. Override with{}",
                    p.dim, p.reset
                );
                eprintln!(
                    "{}SINTERACTIVE_QUOTA_FILE, SINTERACTIVE_QUOTA_HOSTS and{}",
                    p.dim, p.reset
                );
                eprintln!("{}SINTERACTIVE_QUOTA_PORT.{}", p.dim, p.reset);
            } else {
                eprintln!(
                    "{}No cached probe yet; run 'sinteractive quota --check'.{}",
                    p.dim, p.reset
                );
            }
        }
        return Ok(1);
    };

    if args.json {
        // The cache file verbatim, as 0.x `cat` it.
        let text = fs::read_to_string(ctx.state.quota_file())?;
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
        return Ok(0);
    }

    let p = ctx.palette(1);
    // Wording matched to the quota notice and quoted verbatim in
    // skills/hpc-storage/bodhi.md — colour is the only thing added here.
    if snap.over {
        println!(
            "{}{}OVER QUOTA:{}{} {} of {} used ({}%), over by {}{}",
            p.err,
            p.bold,
            p.reset,
            p.err,
            kb_to_size(snap.used_kb),
            kb_to_size(snap.hard_kb),
            snap.pct,
            kb_to_size(snap.over_kb),
            p.reset
        );
    } else {
        println!(
            "{}{}Quota OK:{} {} of {} used ({}%)",
            p.ok,
            p.bold,
            p.reset,
            kb_to_size(snap.used_kb),
            kb_to_size(snap.hard_kb),
            snap.pct
        );
    }
    Ok(0)
}
