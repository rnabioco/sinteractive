//! Linked git worktrees whose work has landed: found, classified, pruned.
//!
//! A worktree is **removable** only when its work is provably on the default
//! branch (HEAD is an ancestor of `origin/HEAD`, or a merged pull request
//! had exactly this HEAD as its head commit) and nothing in it can be lost:
//! it is clean, unlocked, and not the directory the caller is standing in.
//! Everything else is reported and left alone. Nothing here deletes
//! without being told to ([`prune`] with `delete`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::scratch::dir_bytes;
use crate::state::atomic_write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// HEAD is an ancestor of the default branch (and not its tip).
    MergedAncestor,
    /// A merged pull request's head commit is this HEAD (squash/rebase merges).
    MergedPr,
    /// HEAD is the default branch's tip: nothing done here yet.
    Fresh,
    /// Has an upstream or a remote branch of its name.
    Open,
    /// Commits on a branch that exists only here.
    Local,
    /// No branch.
    Detached,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::MergedAncestor => "merged-ancestor",
            State::MergedPr => "merged-pr",
            State::Fresh => "fresh",
            State::Open => "open",
            State::Local => "local",
            State::Detached => "detached",
        }
    }
}

/// What was observed about one worktree; [`classify`] turns it into a state.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub branch: Option<String>,
    pub ancestor: bool,
    pub at_default_tip: bool,
    pub pr_merged: bool,
    pub published: bool,
    pub dirty: bool,
    pub unpushed: bool,
    pub locked: bool,
    pub current: bool,
}

/// The state, and whether the worktree may be removed.
pub fn classify(f: &Facts) -> (State, bool) {
    let state = if f.branch.is_none() {
        State::Detached
    } else if f.at_default_tip {
        State::Fresh
    } else if f.ancestor {
        State::MergedAncestor
    } else if f.pr_merged {
        State::MergedPr
    } else if f.published {
        State::Open
    } else {
        State::Local
    };
    let merged = matches!(state, State::MergedAncestor | State::MergedPr);
    let unpushed = state == State::MergedPr && f.unpushed;
    let removable = merged && !f.dirty && !unpushed && !f.locked && !f.current;
    (state, removable)
}

/// Whether a `gh pr list --json headRefOid` array holds a head commit `sha`.
pub fn pr_head_matches(json: &str, sha: &str) -> bool {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .map(|prs| {
            prs.iter()
                .any(|p| p.get("headRefOid").and_then(|v| v.as_str()) == Some(sha))
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Entry {
    pub path: String,
    pub repo: String,
    pub branch: Option<String>,
    pub state: State,
    pub removable: bool,
    pub bytes: u64,
    pub age_secs: i64,
    pub dirty: bool,
    pub unpushed: bool,
    pub locked: bool,
    pub current: bool,
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    git(dir, args).is_some()
}

/// Linked worktrees of `repo`, the main one excluded.
pub fn of_repo(repo: &Path) -> Vec<PathBuf> {
    let Some(out) = git(repo, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

/// `<base>/<repo>/<name>` directories that hold a `.git` file.
pub fn under_base(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(repos) = fs::read_dir(base) else {
        return out;
    };
    for repo in repos.flatten() {
        let Ok(names) = fs::read_dir(repo.path()) else {
            continue;
        };
        for name in names.flatten() {
            let p = name.path();
            if p.join(".git").is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Where to look: `repo` if given, else `base` if it exists, else the
/// repository containing `cwd`.
pub fn discover(repo: Option<&Path>, base: Option<&Path>, cwd: &Path) -> Vec<PathBuf> {
    if let Some(r) = repo {
        return of_repo(r);
    }
    if let Some(b) = base.filter(|b| b.is_dir()) {
        return under_base(b);
    }
    of_repo(cwd)
}

/// The ref to compare against: `origin/HEAD`, else a local `main`/`master`.
fn default_ref(wt: &Path) -> Option<String> {
    ["origin/HEAD", "main", "master"]
        .into_iter()
        .find(|r| git_ok(wt, &["rev-parse", "--verify", "-q", r]))
        .map(String::from)
}

fn is_locked(wt: &Path) -> bool {
    fs::read_to_string(wt.join(".git"))
        .ok()
        .and_then(|s| s.strip_prefix("gitdir:").map(|g| PathBuf::from(g.trim())))
        .is_some_and(|gitdir| gitdir.join("locked").exists())
}

fn merged_pr(wt: &Path, branch: &str, sha: &str) -> bool {
    let Ok(out) = Command::new("gh")
        .current_dir(wt)
        .args([
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "merged",
            "--json",
            "headRefOid",
            "--limit",
            "5",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    out.status.success() && pr_head_matches(&String::from_utf8_lossy(&out.stdout), sha)
}

/// Whether the branch's reflog holds anything but its creation.
fn has_own_work(wt: &Path, branch: &str) -> bool {
    git(
        wt,
        &[
            "reflog",
            "show",
            "--format=%gs",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_some_and(|log| log.lines().any(|l| !l.starts_with("branch: Created")))
}

/// Look at one worktree. `use_pr` allows the `gh` lookup; `cwd` marks the
/// worktree the caller stands in as not removable.
pub fn inspect(wt: &Path, now: i64, use_pr: bool, cwd: &Path) -> Option<Entry> {
    let common = git(
        wt,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let repo = Path::new(&common).parent()?.to_path_buf();
    let branch = git(wt, &["symbolic-ref", "--short", "-q", "HEAD"]);
    let sha = git(wt, &["rev-parse", "HEAD"])?;
    let default = default_ref(wt);

    let mut facts = Facts {
        branch: branch.clone(),
        dirty: git(wt, &["status", "--porcelain"]).is_none_or(|s| !s.is_empty()),
        locked: is_locked(wt),
        current: cwd.starts_with(wt),
        ..Facts::default()
    };
    if let Some(d) = &default {
        // At the tip is `fresh` only if the branch never got a commit of its
        // own; a fast-forward merge also leaves HEAD at the tip.
        facts.at_default_tip = git(wt, &["rev-parse", d]).as_deref() == Some(sha.as_str())
            && !branch.as_deref().is_some_and(|b| has_own_work(wt, b));
        facts.ancestor = git_ok(wt, &["merge-base", "--is-ancestor", "HEAD", d]);
    }
    if let Some(b) = &branch {
        facts.published = git_ok(wt, &["rev-parse", "--verify", "-q", "@{u}"])
            || git_ok(
                wt,
                &[
                    "rev-parse",
                    "--verify",
                    "-q",
                    &format!("refs/remotes/origin/{b}"),
                ],
            );
        if use_pr && !facts.ancestor && !facts.at_default_tip {
            facts.pr_merged = merged_pr(wt, b, &sha);
            facts.unpushed = git(wt, &["rev-list", "--count", "HEAD", "--not", "--remotes"])
                .is_none_or(|n| n != "0");
        }
    }
    let (state, removable) = classify(&facts);
    let mtime = fs::metadata(wt.join(".git"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(now, |d| d.as_secs() as i64);
    Some(Entry {
        path: wt.to_string_lossy().into_owned(),
        repo: repo.to_string_lossy().into_owned(),
        branch,
        state,
        removable,
        bytes: dir_bytes(wt),
        age_secs: (now - mtime).max(0),
        dirty: facts.dirty,
        unpushed: facts.unpushed,
        locked: facts.locked,
        current: facts.current,
    })
}

/// Every discovered worktree, removable ones first, then biggest first.
pub fn list(
    repo: Option<&Path>,
    base: Option<&Path>,
    cwd: &Path,
    now: i64,
    use_pr: bool,
) -> Vec<Entry> {
    let mut out: Vec<Entry> = discover(repo, base, cwd)
        .iter()
        .filter_map(|p| inspect(p, now, use_pr, cwd))
        .collect();
    out.sort_by(|a, b| {
        b.removable
            .cmp(&a.removable)
            .then(b.bytes.cmp(&a.bytes))
            .then(a.path.cmp(&b.path))
    });
    out
}

pub fn reclaimable_bytes(entries: &[Entry]) -> u64 {
    entries
        .iter()
        .filter(|e| e.removable)
        .map(|e| e.bytes)
        .sum()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Pruned {
    pub path: String,
    pub branch: Option<String>,
    pub bytes: u64,
    pub age_secs: i64,
    pub removed: bool,
    pub error: Option<String>,
}

/// The removable entries at least `older_than_secs` old; with `delete`,
/// removes each worktree (never `--force`) and its branch. One failure is
/// recorded on its entry and does not stop the others.
pub fn prune(entries: &[Entry], older_than_secs: i64, delete: bool) -> Vec<Pruned> {
    entries
        .iter()
        .filter(|e| e.removable && e.age_secs >= older_than_secs)
        .map(|e| {
            let mut p = Pruned {
                path: e.path.clone(),
                branch: e.branch.clone(),
                bytes: e.bytes,
                age_secs: e.age_secs,
                removed: false,
                error: None,
            };
            if delete {
                let repo = Path::new(&e.repo);
                let out = Command::new("git")
                    .arg("-C")
                    .arg(repo)
                    .args(["worktree", "remove", &e.path])
                    .stdin(Stdio::null())
                    .output();
                match out {
                    Ok(o) if o.status.success() => {
                        p.removed = true;
                        if let Some(b) = &e.branch {
                            // Landed work was verified above; a branch that
                            // will not delete is harmless, so not an error.
                            let _ = git(repo, &["branch", "-D", b]);
                        }
                    }
                    Ok(o) => {
                        p.error = Some(String::from_utf8_lossy(&o.stderr).trim().to_string());
                    }
                    Err(e) => p.error = Some(e.to_string()),
                }
            }
            p
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub generated_epoch: i64,
    pub reclaimable_bytes: u64,
    pub worktrees: Vec<Entry>,
}

/// `<scratch root>/worktree-audit/latest.json`.
pub fn report_path(scratch_root: &Path) -> PathBuf {
    scratch_root.join("worktree-audit").join("latest.json")
}

pub fn write_report(path: &Path, entries: Vec<Entry>, now: i64) -> io::Result<Report> {
    let report = Report {
        generated_epoch: now,
        reclaimable_bytes: reclaimable_bytes(&entries),
        worktrees: entries,
    };
    let mut body = serde_json::to_string_pretty(&report).map_err(io::Error::other)?;
    body.push('\n');
    atomic_write(path, body.as_bytes())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged() -> Facts {
        Facts {
            branch: Some("feat".into()),
            ancestor: true,
            ..Facts::default()
        }
    }

    #[test]
    fn classify_ancestor_merged_is_removable() {
        assert_eq!(classify(&merged()), (State::MergedAncestor, true));
    }

    #[test]
    fn classify_dirty_is_never_removable() {
        let f = Facts {
            dirty: true,
            ..merged()
        };
        assert_eq!(classify(&f), (State::MergedAncestor, false));
    }

    #[test]
    fn classify_unmerged_branch_is_local() {
        let f = Facts {
            branch: Some("feat".into()),
            ..Facts::default()
        };
        assert_eq!(classify(&f), (State::Local, false));
        let f = Facts {
            published: true,
            ..f
        };
        assert_eq!(classify(&f), (State::Open, false));
    }

    #[test]
    fn classify_fresh_and_detached_are_never_removable() {
        let f = Facts {
            at_default_tip: true,
            ..merged()
        };
        assert_eq!(classify(&f), (State::Fresh, false));
        let f = Facts {
            branch: None,
            ..merged()
        };
        assert_eq!(classify(&f), (State::Detached, false));
    }

    #[test]
    fn classify_pr_merged_requires_matching_head_sha() {
        let json = r#"[{"headRefOid":"abc"},{"headRefOid":"def"}]"#;
        assert!(pr_head_matches(json, "def"));
        assert!(!pr_head_matches(json, "zzz"));
        let f = Facts {
            branch: Some("feat".into()),
            pr_merged: true,
            ..Facts::default()
        };
        assert_eq!(classify(&f), (State::MergedPr, true));
        let f = Facts {
            unpushed: true,
            ..f
        };
        assert_eq!(classify(&f), (State::MergedPr, false));
    }

    #[test]
    fn classify_locked_and_current_are_never_removable() {
        for f in [
            Facts {
                locked: true,
                ..merged()
            },
            Facts {
                current: true,
                ..merged()
            },
        ] {
            assert!(!classify(&f).1);
        }
    }

    #[test]
    fn pr_lookup_failure_falls_through() {
        assert!(!pr_head_matches("", "abc"));
        assert!(!pr_head_matches("not json", "abc"));
        assert!(!pr_head_matches(r#"{"message":"x"}"#, "abc"));
    }

    #[test]
    fn audit_writes_report_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = report_path(dir.path());
        let e = Entry {
            path: "/w/a".into(),
            repo: "/r".into(),
            branch: Some("feat".into()),
            state: State::MergedAncestor,
            removable: true,
            bytes: 10,
            age_secs: 5,
            dirty: false,
            unpushed: false,
            locked: false,
            current: false,
        };
        let r = write_report(&path, vec![e.clone()], 100).unwrap();
        assert_eq!(r.reclaimable_bytes, 10);
        let back: Report = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(!path.with_extension("json.tmp").exists());
    }
}
