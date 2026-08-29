//! `sinteractive claude hook session-start|prompt|worktree-create|worktree-remove`
//! — Claude Code hook entry points.
//!
//! `session-start` and `prompt` are native replacements for
//! `claude/hooks/sinteractive-session-context.sh` and
//! `sinteractive-walltime-guard.sh`. Claude Code adds a `SessionStart` or
//! `UserPromptSubmit` hook's plain stdout to the agent's context, so the
//! output is prose, not JSON.
//!
//! Both **always exit 0**: a nonzero hook puts an error notice in the
//! transcript, and every reason these can bail — not in a session, no state
//! file, scheduler unreachable — is a normal condition, not a failure.
//!
//! `worktree-create` and `worktree-remove` are different: they *replace*
//! Claude Code's own `git worktree` logic (the `WorktreeCreate` and
//! `WorktreeRemove` events), so that worktrees land on the cluster's scratch
//! filesystem rather than inside the checkout — see [`worktree_root`]. Their
//! exit code is the answer: nonzero aborts the creation.
//!
//! `prompt` is silent above the threshold (`SINTERACTIVE_AGENT_WARN`,
//! default 1800 s), which is the common case: no output, no scheduler
//! traffic. It prefers the session's cached state file, aged exactly
//! (`remaining_seconds - (now - updated_epoch)`), and only asks Slurm when
//! the cache is missing or older than two minutes. `UserPromptSubmit` rather
//! than `PreToolUse`: once per turn is the right cadence for "can this
//! finish?", and it keeps the check off the path of every Bash call.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context as _, Result};
use serde_json::Value;
use sint_core::now_epoch;
use sint_core::session::SessionInfo;

use super::common::Ctx;
use crate::cli::{HookArgs, HookEvent};

pub fn run(args: HookArgs) -> Result<i32> {
    match args.event {
        HookEvent::SessionStart => session_start(),
        HookEvent::Prompt => prompt(),
        HookEvent::WorktreeCreate => worktree_create(),
        HookEvent::WorktreeRemove => worktree_remove(),
    }
}

/// The agent briefing, when inside a session; silence otherwise.
fn session_start() -> Result<i32> {
    let ctx = Ctx::new();
    if ctx.cfg.job_id.is_none() {
        return Ok(0);
    }
    // agent-context prints its own errors on stderr and exits 1 when the
    // job is gone; the hook still exits 0.
    let _ = super::agent_context::run();
    Ok(0)
}

/// `(remaining_seconds, end_epoch)` for the current session, or `None`.
fn budget(ctx: &Ctx, job_id: u64, now: i64) -> Option<(i64, Option<i64>)> {
    if let Some(state) = ctx.state.read_state(job_id) {
        if let Some(rem) = state.aged_remaining(now) {
            return Some((rem, state.end_epoch));
        }
    }
    let row = ctx.slurm.job(job_id).ok().flatten()?;
    let info = SessionInfo::from_row(&row, now);
    let rem = info.remaining_seconds?;
    Some((rem, info.end_epoch))
}

/// Text of the warning, or `None` when there is nothing to say.
pub fn walltime_warning(
    job_id: u64,
    remaining: i64,
    end_epoch: Option<i64>,
    warn_at: i64,
) -> Option<String> {
    let remaining = remaining.max(0);
    if remaining > warn_at {
        return None;
    }
    let left = if remaining >= 3600 {
        format!("{}h {}m", remaining / 3600, (remaining % 3600) / 60)
    } else if remaining >= 60 {
        format!("{}m", remaining / 60)
    } else {
        format!("{remaining}s")
    };
    let ends = end_epoch
        .map(|e| format!(" (ends {})", local_hhmm(e)))
        .unwrap_or_default();
    Some(format!(
        "Walltime warning: this sinteractive session (job {job_id}) has\n\
         {left} left{ends}. It self-terminates ~10s before the limit, which ends this\n\
         shell and anything attached to it, including an srun you are streaming from.\n\
         \n\
         Do not start work that cannot finish in that window. Long work belongs in its\n\
         own allocation with its own -t (salloc --no-shell + srun --overlap), which\n\
         survives independently — or ask the user for a fresh session.\n"
    ))
}

/// `HH:MM` local time for an epoch.
fn local_hhmm(epoch: i64) -> String {
    let s = sint_core::notices::format_local_datetime(epoch);
    // `%a %b %-d %H:%M` — the time is the last word.
    s.rsplit(' ').next().unwrap_or("").to_string()
}

fn prompt() -> Result<i32> {
    let ctx = Ctx::new();
    let Some(job_id) = ctx.cfg.job_id else {
        return Ok(0);
    };
    let now = now_epoch();
    let Some((remaining, end_epoch)) = budget(&ctx, job_id, now) else {
        return Ok(0);
    };
    if let Some(text) = walltime_warning(job_id, remaining, end_epoch, ctx.cfg.agent_warn) {
        print!("{text}");
    }
    Ok(0)
}

// ---- worktrees --------------------------------------------------------------

/// The JSON Claude Code writes to the hook's stdin.
fn hook_input() -> Result<Value> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .context("read the hook input")?;
    if text.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(&text).context("the hook input is not JSON")
}

fn field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// `git` in `dir`, its stdout trimmed; an error carrying stderr otherwise.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Where a repository's worktrees go: `SINTERACTIVE_WORKTREES/<repo>`, else
/// `/scratch/alpine/$USER/worktrees/<repo>` where that scratch exists
/// (Alpine: `/projects` is the small, backed-up tier and a worktree is a
/// throwaway build tree), else Claude Code's own `<repo>/.claude/worktrees`
/// — on a cluster with one filesystem, such as Bodhi, that is fine as it is.
pub fn worktree_root(repo: &Path, configured: Option<&Path>, user: &str) -> PathBuf {
    let name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into());
    if let Some(root) = configured {
        return root.join(name);
    }
    let scratch = PathBuf::from("/scratch/alpine").join(user);
    if scratch.is_dir() {
        return scratch.join("worktrees").join(name);
    }
    repo.join(".claude").join("worktrees")
}

/// `WorktreeCreate`: make `<root>/<name>` a worktree of the repository
/// Claude Code is running in, on a branch `worktree-<name>` from the
/// remote's default branch (as Claude Code's own `fresh` base does), and
/// print its path. An existing directory of that name is reused as it is.
fn worktree_create() -> Result<i32> {
    let input = hook_input()?;
    let base = field(&input, "base_path")
        .or_else(|| field(&input, "cwd"))
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let repo = PathBuf::from(git(&base, &["rev-parse", "--show-toplevel"])?);
    let name = match field(&input, "name") {
        Some(n) => n.to_string(),
        None => format!("wt-{}", now_epoch()),
    };
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        bail!("worktree name {name:?} is not a plain directory name");
    }
    let ctx = Ctx::new();
    let user = std::env::var("USER").unwrap_or_default();
    let root = worktree_root(&repo, ctx.cfg.worktrees.as_deref(), &user);
    let path = root.join(&name);
    if path.exists() {
        println!("{}", path.display());
        return Ok(0);
    }
    std::fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;

    let branch = format!("worktree-{name}");
    let has_branch = git(
        &repo,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok();
    let path_s = path.to_string_lossy().into_owned();
    if has_branch {
        git(&repo, &["worktree", "add", &path_s, &branch])?;
    } else {
        // A fresh base: the remote's default branch, brought up to date
        // within a few seconds when the network allows, else as cached;
        // local HEAD when there is no remote to ask.
        if git(&repo, &["remote", "get-url", "origin"]).is_ok() {
            let _ = Command::new("timeout")
                .args(["5", "git", "-C"])
                .arg(&repo)
                .args(["fetch", "-q", "origin"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let base_ref = git(&repo, &["rev-parse", "--verify", "-q", "origin/HEAD"])
            .map(|_| "origin/HEAD".to_string())
            .unwrap_or_else(|_| "HEAD".to_string());
        git(
            &repo,
            &["worktree", "add", "-b", &branch, &path_s, &base_ref],
        )?;
    }
    println!("{}", path.display());
    Ok(0)
}

/// `WorktreeRemove`: remove the worktree at the input's `path`, and its
/// `worktree-*` branch with it, the way Claude Code's own removal does.
fn worktree_remove() -> Result<i32> {
    let input = hook_input()?;
    let path = field(&input, "path")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("the hook input names no path"))?;
    if !path.exists() {
        return Ok(0);
    }
    let branch = git(&path, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    let common = git(
        &path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let repo = Path::new(&common)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("{common} has no parent"))?;
    git(
        &repo,
        &["worktree", "remove", "--force", &path.to_string_lossy()],
    )?;
    if let Some(b) = branch.filter(|b| b.starts_with("worktree-")) {
        let _ = git(&repo, &["branch", "-D", &b]);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktrees_go_to_scratch_when_there_is_one() {
        let repo = Path::new("/projects/me/sinteractive");
        assert_eq!(
            worktree_root(repo, Some(Path::new("/x/wt")), "me"),
            PathBuf::from("/x/wt/sinteractive")
        );
        // No scratch for this user (the test host has none): the stock place.
        assert_eq!(
            worktree_root(repo, None, "nobody-has-this-scratch-dir"),
            PathBuf::from("/projects/me/sinteractive/.claude/worktrees")
        );
    }

    #[test]
    fn silent_above_threshold() {
        assert!(walltime_warning(1, 1801, None, 1800).is_none());
        assert!(walltime_warning(1, 1800, None, 1800).is_some());
    }

    #[test]
    fn wording_and_units() {
        let t = walltime_warning(147845, 3725, None, 7200).unwrap();
        assert!(t.starts_with(
            "Walltime warning: this sinteractive session (job 147845) has\n1h 2m left."
        ));
        let t = walltime_warning(1, 125, None, 1800).unwrap();
        assert!(t.contains("\n2m left."));
        let t = walltime_warning(1, 59, None, 1800).unwrap();
        assert!(t.contains("\n59s left."));
        let t = walltime_warning(1, -5, None, 1800).unwrap();
        assert!(t.contains("\n0s left."));
        assert!(t.contains("salloc --no-shell + srun --overlap"));
    }

    #[test]
    fn ends_clause_is_hhmm() {
        let t = walltime_warning(1, 600, Some(1788422100), 1800).unwrap();
        let ends = t.lines().nth(1).unwrap();
        assert!(ends.contains("left (ends "), "{ends}");
        let i = ends.find("(ends ").unwrap() + "(ends ".len();
        let hhmm = &ends[i..i + 5];
        assert_eq!(&hhmm[2..3], ":", "{hhmm}");
        assert_eq!(&ends[i + 5..i + 7], ").");
    }
}
