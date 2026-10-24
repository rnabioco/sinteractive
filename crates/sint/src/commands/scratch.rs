//! `sinteractive scratch new|ls|clean` — scratch directories with a
//! manifest, so working files have an owner and an age and cleanup is one
//! command rather than an audit.
//!
//! `clean` lists what it would remove unless `--yes` is given, and only ever
//! touches directories that carry a manifest (see [`sint_core::scratch`]).

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::Serialize;
use sint_core::quota::kb_to_size;
use sint_core::scratch::{self, Cleaned, Entry, Origin};

use super::common::{eprint_error, Ctx};
use crate::cli::{ScratchArgs, ScratchCommand};

/// What `scratch new` made or found.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ScratchDir {
    pub topic: String,
    pub path: String,
    /// False when the directory already existed.
    pub created: bool,
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

fn age(s: &str) -> Result<i64> {
    scratch::parse_age(s).ok_or_else(|| anyhow!("bad age '{s}': use e.g. 14d, 36h, 2w"))
}

/// `scratch new`, for the CLI and the MCP tool. The manifest records the git
/// checkout and branch of the current directory and the session, if any.
pub fn new_data(ctx: &Ctx, topic: &str, keep_for: Option<&str>) -> Result<ScratchDir> {
    let keep = keep_for.map(age).transpose()?;
    let user = std::env::var("USER").unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let origin = Origin::detect(&user, ctx.cfg.job_id, &cwd);
    let root = scratch_root()?;
    std::fs::create_dir_all(&root)?;
    let (path, created) = scratch::create(&root, topic, &origin, sint_core::now_epoch(), keep)?;
    Ok(ScratchDir {
        topic: topic.to_string(),
        path: path.to_string_lossy().into_owned(),
        created,
    })
}

fn size(bytes: u64) -> String {
    kb_to_size(bytes / 1024)
}

fn days(secs: i64) -> String {
    format!("{}d", secs / 86400)
}

fn print_ls(entries: &[Entry]) {
    if entries.is_empty() {
        println!(
            "no scratch directories under {}",
            scratch_root().unwrap_or_default().display()
        );
        return;
    }
    for e in entries {
        let branch = match (&e.branch, e.branch_exists) {
            (Some(b), Some(false)) => format!("{b} (gone)"),
            (Some(b), _) => b.clone(),
            (None, _) => "-".to_string(),
        };
        println!(
            "{:>9} {:>5}  {:<24} {}",
            size(e.bytes),
            days(e.age_secs),
            e.topic,
            branch
        );
    }
}

fn print_clean(done: &[Cleaned], yes: bool) {
    if done.is_empty() {
        println!("nothing to clean");
        return;
    }
    let total: u64 = done.iter().map(|c| c.bytes).sum();
    for c in done {
        println!("{:>9} {:>5}  {}", size(c.bytes), days(c.age_secs), c.path);
    }
    if yes {
        println!("removed {} directories, {}", done.len(), size(total));
    } else {
        println!(
            "would remove {} directories, {}; pass --yes to do it",
            done.len(),
            size(total)
        );
    }
}

pub fn run(args: ScratchArgs) -> Result<i32> {
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
        ScratchCommand::New {
            topic,
            keep_for,
            json,
        } => match new_data(&ctx, &topic, keep_for.as_deref()) {
            Ok(d) if json => println!("{}", serde_json::to_string(&d)?),
            Ok(d) => println!("{}", d.path),
            Err(e) => return fail(e, json),
        },
        ScratchCommand::Ls { json } => {
            let entries = scratch::list(&scratch_root()?, sint_core::now_epoch());
            if json {
                println!("{}", serde_json::to_string(&entries)?);
            } else {
                print_ls(&entries);
            }
        }
        ScratchCommand::Clean {
            older_than,
            yes,
            json,
        } => {
            let secs = match age(&older_than) {
                Ok(s) => s,
                Err(e) => return fail(e, json),
            };
            let done = scratch::clean(&scratch_root()?, secs, sint_core::now_epoch(), yes)?;
            if json {
                println!("{}", serde_json::to_string(&done)?);
            } else {
                print_clean(&done, yes);
            }
        }
    }
    Ok(0)
}
