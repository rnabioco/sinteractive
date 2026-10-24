//! `sinteractive worktree ls|prune|audit` — worktrees whose work has landed.
//!
//! `prune` lists what it would remove unless `--yes` is given. `audit` only
//! writes a report (see [`sint_core::worktree`]) and, with `--resubmit`,
//! queues the next audit as a Slurm job; it never deletes.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use sint_core::quota::kb_to_size;
use sint_core::scratch;
use sint_core::slurm::Slurm;
use sint_core::worktree::{self, Entry, Pruned};

use super::common::{eprint_error, Ctx};
use crate::cli::{WorktreeArgs, WorktreeCommand};

/// Slurm job name and comment of the periodic audit.
pub const AUDIT_JOB: &str = "worktree-audit";

fn size(bytes: u64) -> String {
    kb_to_size(bytes / 1024)
}

fn days(secs: i64) -> String {
    format!("{}d", secs / 86400)
}

fn age(s: &str) -> Result<i64> {
    scratch::parse_age(s).ok_or_else(|| anyhow!("bad age '{s}': use e.g. 14d, 36h, 2w"))
}

/// Where `<repo>/<name>` worktrees are kept by the create hook.
fn worktree_base(ctx: &Ctx) -> Option<PathBuf> {
    if let Some(c) = &ctx.cfg.worktrees {
        return Some(c.clone());
    }
    let user = std::env::var("USER").unwrap_or_default();
    Some(
        PathBuf::from("/scratch/alpine")
            .join(user)
            .join("worktrees"),
    )
}

fn scratch_root() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(scratch::root(
        &home,
        &std::env::var("USER").unwrap_or_default(),
    ))
}

fn entries(ctx: &Ctx, repo: Option<&Path>, use_pr: bool) -> Vec<Entry> {
    let cwd = std::env::current_dir().unwrap_or_default();
    worktree::list(
        repo,
        worktree_base(ctx).as_deref(),
        &cwd,
        sint_core::now_epoch(),
        use_pr,
    )
}

fn print_ls(entries: &[Entry]) {
    if entries.is_empty() {
        println!("no worktrees found");
        return;
    }
    for e in entries {
        let mut state = e.state.as_str().to_string();
        for (on, tag) in [
            (e.dirty, "dirty"),
            (e.locked, "locked"),
            (e.current, "here"),
        ] {
            if on {
                state.push_str(&format!("+{tag}"));
            }
        }
        println!(
            "{} {:>9} {:>5}  {:<22} {:<24} {}",
            if e.removable { "x" } else { " " },
            size(e.bytes),
            days(e.age_secs),
            state,
            e.branch.as_deref().unwrap_or("-"),
            e.path
        );
    }
    println!(
        "x = removable; {} reclaimable",
        size(worktree::reclaimable_bytes(entries))
    );
}

fn print_prune(done: &[Pruned], yes: bool) {
    if done.is_empty() {
        println!("nothing to prune");
        return;
    }
    for p in done {
        let mark = match (&p.error, p.removed) {
            (Some(_), _) => "FAILED",
            (None, true) => "removed",
            (None, false) => "",
        };
        println!(
            "{:>9} {:>5}  {:<8} {}",
            size(p.bytes),
            days(p.age_secs),
            mark,
            p.path
        );
        if let Some(err) = &p.error {
            println!("          {err}");
        }
    }
    let total: u64 = done.iter().filter(|p| p.removed).map(|p| p.bytes).sum();
    if yes {
        println!(
            "removed {} of {} worktrees, {}",
            done.iter().filter(|p| p.removed).count(),
            done.len(),
            size(total)
        );
    } else {
        let all: u64 = done.iter().map(|p| p.bytes).sum();
        println!(
            "would remove {} worktrees, {}; pass --yes to do it",
            done.len(),
            size(all)
        );
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Queue the next audit unless one is already pending or running (other
/// than the job this is). Returns what to tell the user.
fn resubmit(repo: Option<&Path>, every_secs: i64, every: &str, log_dir: &Path) -> Result<String> {
    let slurm = Slurm::new();
    let me: Option<u64> = std::env::var("SLURM_JOB_ID")
        .ok()
        .and_then(|s| s.parse().ok());
    let queued = slurm.my_job_names(&["--states", "PENDING,RUNNING"]);
    if let Some((id, _)) = queued
        .iter()
        .find(|(id, name)| name.as_str() == AUDIT_JOB && Some(**id) != me)
    {
        return Ok(format!(
            "an audit is already queued (job {id}); not submitting another"
        ));
    }
    let exe = std::env::current_exe()?;
    let mut cmd = format!(
        "{} worktree audit --resubmit --every {}",
        shell_quote(&exe.to_string_lossy()),
        shell_quote(every)
    );
    if let Some(r) = repo {
        cmd.push_str(&format!(" --repo {}", shell_quote(&r.to_string_lossy())));
    }
    let args: Vec<String> = [
        "-J",
        AUDIT_JOB,
        "--comment",
        AUDIT_JOB,
        "-t",
        "00:30:00",
        "-c",
        "1",
        "--mem",
        "1G",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([
        format!("--begin=now+{every_secs}seconds"),
        "-o".to_string(),
        log_dir.join("audit-%j.log").to_string_lossy().into_owned(),
        "--wrap".to_string(),
        cmd,
    ])
    .collect();
    let id = slurm.sbatch(&args)?;
    Ok(format!("next audit queued as job {id}, in {every}"))
}

pub fn run(args: WorktreeArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let fail = |e: anyhow::Error, json: bool| -> Result<i32> {
        if json {
            println!("{}", serde_json::json!({ "error": format!("{e:#}") }));
        } else {
            eprint_error(&ctx.palette(2), &format!("{e:#}"));
        }
        Ok(1)
    };
    match args.command {
        WorktreeCommand::Ls { repo, no_pr, json } => {
            let es = entries(&ctx, repo.as_deref(), !no_pr);
            if json {
                println!("{}", serde_json::to_string(&es)?);
            } else {
                print_ls(&es);
            }
        }
        WorktreeCommand::Prune {
            repo,
            older_than,
            no_pr,
            yes,
            json,
        } => {
            let secs = match age(&older_than) {
                Ok(s) => s,
                Err(e) => return fail(e, json),
            };
            let es = entries(&ctx, repo.as_deref(), !no_pr);
            let done = worktree::prune(&es, secs, yes);
            if json {
                println!("{}", serde_json::to_string(&done)?);
            } else {
                print_prune(&done, yes);
            }
            if done.iter().any(|p| p.error.is_some()) {
                return Ok(1);
            }
        }
        WorktreeCommand::Audit {
            repo,
            resubmit: again,
            every,
        } => {
            let every_secs = match age(&every) {
                Ok(s) => s,
                Err(e) => return fail(e, false),
            };
            let root = scratch_root()?;
            let path = worktree::report_path(&root);
            let es = entries(&ctx, repo.as_deref(), true);
            let report = match worktree::write_report(&path, es, sint_core::now_epoch()) {
                Ok(r) => r,
                Err(e) => return fail(e.into(), false),
            };
            println!(
                "{} worktrees, {} removable, {} reclaimable; report: {}",
                report.worktrees.len(),
                report.worktrees.iter().filter(|e| e.removable).count(),
                size(report.reclaimable_bytes),
                path.display()
            );
            if again {
                let log_dir = path.parent().unwrap_or(&root);
                match resubmit(repo.as_deref(), every_secs, &every, log_dir) {
                    Ok(msg) => println!("{msg}"),
                    Err(e) => eprintln!("warning: could not queue the next audit: {e:#}"),
                }
            }
        }
    }
    Ok(0)
}
