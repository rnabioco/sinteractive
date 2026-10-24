//! `sinteractive quota [--check] [--json]` — storage quota (Bodhi).
//!
//! `quota` reports the cached probe; `quota --check` probes now, rewrites
//! the cache, pokes every live session so the fresh answer reaches their
//! notices on the next tick, and reports. Ports `check_quota_cmd` (script
//! lines 1437-1476). Exits 0 whether or not the user is over — being over
//! quota is a fact to report, not an error in the check.

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use sint_core::quota::{self, kb_to_size, QuotaSnapshot};
use sint_core::usage::{self, UsageReport};

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

/// The directory the usage index covers.
fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

/// `quota --top`: the cached usage index as a report. An error, not a scan,
/// when there is no index yet.
pub fn top_data(ctx: &Ctx, n: usize) -> Result<UsageReport> {
    let index = usage::load(&ctx.state).ok_or_else(|| {
        anyhow!("no usage index yet; build one with 'sinteractive quota --reindex' (a batch job)")
    })?;
    Ok(usage::report(&index, n, sint_core::now_epoch()))
}

/// Submit the walk as its own job: it is slow and must not run in a session.
fn submit_reindex(ctx: &Ctx, json: bool) -> Result<i32> {
    let exe = std::env::current_exe().context("cannot find the sinteractive binary")?;
    let wrap = format!(
        "'{}' quota --reindex-run",
        exe.display().to_string().replace('\'', "'\\''")
    );
    let args = [
        "-J".to_string(),
        "quota-index".to_string(),
        "--comment=quota-index".to_string(),
        "-t".to_string(),
        "04:00:00".to_string(),
        "-c".to_string(),
        "1".to_string(),
        "--mem=4G".to_string(),
        "--wrap".to_string(),
        wrap,
    ];
    let job_id = ctx.slurm.sbatch(&args)?;
    if json {
        println!(r#"{{"job_id":{job_id},"name":"quota-index"}}"#);
    } else {
        println!(
            "submitted job {job_id} (quota-index); 'sinteractive quota --top' reads the result"
        );
    }
    Ok(0)
}

fn print_top(report: &UsageReport) {
    let size = |b: u64| kb_to_size(b / 1024);
    let age_h = report.age_secs / 3600;
    println!(
        "{} — index {}h old{}",
        report.root,
        age_h,
        if report.stale { " (stale)" } else { "" }
    );
    for d in &report.top {
        println!(
            "{:>9} {:>9} files  {:<12?}  {}",
            size(d.bytes),
            d.files,
            d.kind,
            d.path
        );
    }
    for g in &report.duplicates {
        println!("\n{} each, {} copies:", size(g.bytes), g.paths.len());
        for p in &g.paths {
            println!("  {p}");
        }
    }
}

pub fn run(args: QuotaArgs) -> Result<i32> {
    let ctx = Ctx::new();
    if args.reindex_run {
        let index = usage::build(
            &home_dir()?,
            sint_core::now_epoch(),
            usage::Params::default(),
        );
        usage::write(&ctx.state, &index)?;
        return Ok(0);
    }
    if args.reindex {
        return submit_reindex(&ctx, args.json);
    }
    if let Some(n) = args.top {
        return match top_data(&ctx, n) {
            Ok(report) if args.json => {
                println!("{}", serde_json::to_string(&report)?);
                Ok(0)
            }
            Ok(report) => {
                print_top(&report);
                Ok(0)
            }
            Err(e) if args.json => {
                println!(
                    r#"{{"error":{}}}"#,
                    serde_json::to_string(&format!("{e:#}"))?
                );
                Ok(1)
            }
            Err(e) => {
                eprint_error(&ctx.palette(2), &format!("{e:#}"));
                Ok(1)
            }
        };
    }
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
