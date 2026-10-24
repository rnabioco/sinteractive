//! `sinteractive worktree ls|prune|audit` against real throwaway repositories.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::FakeSlurm;
use predicates::prelude::*;

fn run_git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .status()
        .expect("run git");
    assert!(st.success(), "git {args:?} failed");
}

/// A repository with one commit on `main` and a fake `origin` it is
/// pushed to, so `origin/HEAD` exists.
struct Repo {
    _tmp: tempfile::TempDir,
    main: PathBuf,
}

impl Repo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin.git");
        let main = tmp.path().join("proj");
        std::fs::create_dir_all(&main).unwrap();
        run_git(
            tmp.path(),
            &["init", "-q", "--bare", "-b", "main", "origin.git"],
        );
        run_git(&main, &["init", "-q", "-b", "main"]);
        run_git(&main, &["commit", "-q", "--allow-empty", "-m", "root"]);
        run_git(
            &main,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        run_git(&main, &["push", "-q", "-u", "origin", "main"]);
        run_git(&main, &["remote", "set-head", "origin", "main"]);
        Repo { _tmp: tmp, main }
    }

    /// A linked worktree on a new branch with one commit of its own.
    fn worktree(&self, name: &str) -> PathBuf {
        let path = self._tmp.path().join("wts").join(name);
        run_git(
            &self.main,
            &["worktree", "add", "-q", "-b", name, path.to_str().unwrap()],
        );
        run_git(&path, &["commit", "-q", "--allow-empty", "-m", name]);
        path
    }

    /// Fast-forward `main` (and origin) to `branch`.
    fn land(&self, branch: &str) {
        run_git(&self.main, &["merge", "-q", "--ff-only", branch]);
        run_git(&self.main, &["push", "-q", "origin", "main"]);
    }
}

fn ls_json(fx: &FakeSlurm, repo: &Repo) -> Vec<serde_json::Value> {
    let out = fx
        .sinteractive()
        .args(["worktree", "ls", "--no-pr", "--json", "--repo"])
        .arg(&repo.main)
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

fn find<'a>(es: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    es.iter()
        .find(|e| e["path"].as_str().unwrap().ends_with(name))
        .unwrap_or_else(|| panic!("{name} not listed in {es:?}"))
}

#[test]
fn ls_marks_landed_worktrees_removable_and_leaves_the_rest() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    repo.worktree("done");
    repo.worktree("wip");
    repo.land("done");
    let dirty = repo.worktree("dirty");
    repo.land("dirty");
    std::fs::write(dirty.join("scratch.txt"), "x").unwrap();

    let es = ls_json(&fx, &repo);
    assert_eq!(find(&es, "done")["state"], "merged-ancestor");
    assert_eq!(find(&es, "done")["removable"], true);
    assert_eq!(find(&es, "wip")["state"], "local");
    assert_eq!(find(&es, "wip")["removable"], false);
    assert_eq!(find(&es, "dirty")["removable"], false);
    assert_eq!(find(&es, "dirty")["dirty"], true);
}

#[test]
fn prune_dry_run_removes_nothing() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let done = repo.worktree("done");
    repo.land("done");
    fx.sinteractive()
        .args(["worktree", "prune", "--no-pr", "--repo"])
        .arg(&repo.main)
        .assert()
        .success()
        .stdout(predicate::str::contains("pass --yes"));
    assert!(done.is_dir());
}

#[test]
fn prune_yes_removes_worktree_and_branch() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let done = repo.worktree("done");
    let wip = repo.worktree("wip");
    repo.land("done");
    fx.sinteractive()
        .args(["worktree", "prune", "--no-pr", "--yes", "--repo"])
        .arg(&repo.main)
        .assert()
        .success()
        .stdout(predicate::str::contains("removed 1 of 1"));
    assert!(!done.exists());
    assert!(wip.is_dir());
    let branches = Command::new("git")
        .arg("-C")
        .arg(&repo.main)
        .args(["branch", "--list", "done"])
        .output()
        .unwrap();
    assert!(branches.stdout.is_empty());
}

#[test]
fn prune_keeps_younger_than_older_than() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let done = repo.worktree("done");
    repo.land("done");
    fx.sinteractive()
        .args([
            "worktree",
            "prune",
            "--no-pr",
            "--yes",
            "--older-than",
            "14d",
            "--repo",
        ])
        .arg(&repo.main)
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to prune"));
    assert!(done.is_dir());
}

#[test]
fn a_merged_pr_with_the_same_head_counts_as_landed() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let wt = repo.worktree("squashed");
    let sha = Command::new("git")
        .arg("-C")
        .arg(&wt)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let sha = String::from_utf8(sha.stdout).unwrap().trim().to_string();
    let bin = tempfile::tempdir().unwrap();
    let gh = bin.path().join("gh");
    std::fs::write(
        &gh,
        format!("#!/bin/sh\necho '[{{\"headRefOid\":\"{sha}\"}}]'\n"),
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let out = fx
        .sinteractive()
        .env("PATH", format!("{}:{}", bin.path().display(), fx.path()))
        .args(["worktree", "ls", "--json", "--repo"])
        .arg(&repo.main)
        .output()
        .unwrap();
    let es: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(find(&es, "squashed")["state"], "merged-pr");
    // Pushed head or not, an unpushed commit keeps it.
    assert_eq!(find(&es, "squashed")["unpushed"], true);
    assert_eq!(find(&es, "squashed")["removable"], false);
}

#[test]
fn audit_writes_the_report_without_deleting() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let done = repo.worktree("done");
    repo.land("done");
    fx.sinteractive()
        .args(["worktree", "audit", "--repo"])
        .arg(&repo.main)
        .assert()
        .success()
        .stdout(predicate::str::contains("1 removable"));
    assert!(done.is_dir());
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            fx.home_dir()
                .join("scratch")
                .join("worktree-audit")
                .join("latest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["worktrees"].as_array().unwrap().len(), 1);
    assert!(fx.calls_to("sbatch").is_empty());
}

#[test]
fn audit_resubmit_submits_once_and_skips_when_pending() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let args = ["worktree", "audit", "--resubmit", "--every", "2d", "--repo"];
    fx.sinteractive()
        .args(args)
        .arg(&repo.main)
        .assert()
        .success()
        .stdout(predicate::str::contains("next audit queued"));
    let calls = fx.calls_to("sbatch");
    assert_eq!(calls.len(), 1);
    let call = calls[0].join(" ");
    assert!(call.contains("worktree-audit"), "{call}");
    assert!(call.contains("--begin=now+172800seconds"), "{call}");

    // The fake sbatch made it RUNNING with the comment; a second audit
    // sees a queued one and submits nothing.
    fx.sinteractive()
        .args(args)
        .arg(&repo.main)
        .assert()
        .success()
        .stdout(predicate::str::contains("already queued"));
    assert_eq!(fx.calls_to("sbatch").len(), 1);
}

#[test]
fn audit_submit_failure_is_a_warning() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    fx.write("sbatch.fail", "sbatch: error: Invalid qos\n");
    fx.sinteractive()
        .args(["worktree", "audit", "--resubmit", "--repo"])
        .arg(&repo.main)
        .assert()
        .success()
        .stderr(predicate::str::contains("could not queue the next audit"));
}

#[test]
fn discovery_finds_base_repo_name_layout() {
    let fx = FakeSlurm::new();
    let repo = Repo::new();
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let wt = dir.join("done");
    run_git(
        &repo.main,
        &["worktree", "add", "-q", "-b", "done", wt.to_str().unwrap()],
    );
    run_git(&wt, &["commit", "-q", "--allow-empty", "-m", "done"]);
    repo.land("done");
    let out = fx
        .sinteractive()
        .env("SINTERACTIVE_WORKTREES", base.path())
        .args(["worktree", "ls", "--no-pr", "--json"])
        .output()
        .unwrap();
    let es: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(es.len(), 1);
    assert_eq!(es[0]["removable"], true);
}
