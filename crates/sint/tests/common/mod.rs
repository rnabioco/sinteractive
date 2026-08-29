//! Shared harness for the integration tests: a throwaway fixture for the
//! `tests/fake-slurm/` shims and an [`assert_cmd::Command`] wired to it.
//!
//! Each test gets its own tempdir with `slurm/` (the `FAKE_SLURM_DIR`),
//! `cache/` (`SINTERACTIVE_CACHE`), `home/` (`HOME`) and `claude/`
//! (`CLAUDE_CONFIG_DIR`) so nothing leaks into the developer's account.
//! `PATH` is the shims directory first, then a minimal system path — never
//! the developer's own, so a real `claude`, `squeue` or `jq` on it cannot
//! be picked up by accident.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// Repository root (two levels above `crates/sint`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// `tests/fake-slurm/` at the repo root.
pub fn shims_dir() -> PathBuf {
    repo_root().join("tests/fake-slurm")
}

/// One row of `jobs.tsv`. Defaults describe a running 4-CPU session named
/// `web` on `node01`; override what a test cares about.
#[derive(Debug, Clone)]
pub struct Job {
    pub job_id: u64,
    pub comment: String,
    pub node: String,
    pub partition: String,
    pub elapsed: String,
    pub time_limit: String,
    pub end_time: String,
    pub cpus: u32,
    pub mem: String,
    pub tres: String,
    pub state: String,
    pub reason: String,
    pub start_time: String,
    /// Where the job was submitted from (`AllocNodes` / `AllocSID`); empty
    /// and 0 — nowhere in particular — unless a test says otherwise.
    pub alloc_node: String,
    pub alloc_sid: u32,
}

impl Default for Job {
    fn default() -> Self {
        Job {
            job_id: 147845,
            comment: "sinteractive:web".into(),
            node: "node01".into(),
            partition: "interactive".into(),
            elapsed: "1:02:03".into(),
            time_limit: "8:00:00".into(),
            end_time: "2026-01-01T08:00:00".into(),
            cpus: 4,
            mem: "16G".into(),
            tres: "N/A".into(),
            state: "RUNNING".into(),
            reason: "None".into(),
            start_time: "2026-01-01T00:00:00".into(),
            alloc_node: String::new(),
            alloc_sid: 0,
        }
    }
}

impl Job {
    pub fn new(job_id: u64, comment: &str) -> Self {
        Job {
            job_id,
            comment: comment.into(),
            ..Job::default()
        }
    }

    pub fn node(mut self, node: &str) -> Self {
        self.node = node.into();
        self
    }

    pub fn state(mut self, state: &str) -> Self {
        self.state = state.into();
        self
    }

    pub fn reason(mut self, reason: &str) -> Self {
        self.reason = reason.into();
        self
    }

    pub fn tres(mut self, tres: &str) -> Self {
        self.tres = tres.into();
        self
    }

    pub fn mem(mut self, mem: &str) -> Self {
        self.mem = mem.into();
        self
    }

    pub fn cpus(mut self, cpus: u32) -> Self {
        self.cpus = cpus;
        self
    }

    pub fn partition(mut self, partition: &str) -> Self {
        self.partition = partition.into();
        self
    }

    pub fn end_time(mut self, end_time: &str) -> Self {
        self.end_time = end_time.into();
        self
    }

    pub fn elapsed(mut self, elapsed: &str) -> Self {
        self.elapsed = elapsed.into();
        self
    }

    pub fn time_limit(mut self, time_limit: &str) -> Self {
        self.time_limit = time_limit.into();
        self
    }

    /// Submitted on `node` by a process in session `sid` (0: a process
    /// that has since exited).
    pub fn submitted_from(mut self, node: &str, sid: u32) -> Self {
        self.alloc_node = node.into();
        self.alloc_sid = sid;
        self
    }

    /// The `jobs.tsv` line for this job (no trailing newline).
    pub fn to_tsv(&self) -> String {
        [
            self.job_id.to_string(),
            self.comment.clone(),
            self.node.clone(),
            self.partition.clone(),
            self.elapsed.clone(),
            self.time_limit.clone(),
            self.end_time.clone(),
            self.cpus.to_string(),
            self.mem.clone(),
            self.tres.clone(),
            self.state.clone(),
            self.reason.clone(),
            self.start_time.clone(),
            self.alloc_node.clone(),
            self.alloc_sid.to_string(),
        ]
        .join("\t")
    }
}

/// A fixture directory plus the environment that points a command at it.
pub struct FakeSlurm {
    pub tmp: TempDir,
}

impl FakeSlurm {
    /// Empty queue, `next_id` 1000.
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        for d in ["slurm", "cache", "home", "claude"] {
            fs::create_dir_all(tmp.path().join(d)).expect("fixture dirs");
        }
        let fx = FakeSlurm { tmp };
        fx.write("next_id", "1000\n");
        fx
    }

    /// Queue seeded with `jobs`.
    pub fn with_jobs(jobs: &[Job]) -> Self {
        let fx = FakeSlurm::new();
        fx.seed_jobs(jobs);
        fx
    }

    pub fn dir(&self) -> PathBuf {
        self.tmp.path().join("slurm")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.tmp.path().join("cache")
    }

    pub fn home_dir(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    pub fn claude_dir(&self) -> PathBuf {
        self.tmp.path().join("claude")
    }

    /// Write `contents` to `$FAKE_SLURM_DIR/name`.
    pub fn write(&self, name: &str, contents: &str) {
        fs::write(self.dir().join(name), contents).expect("write fixture");
    }

    /// Read `$FAKE_SLURM_DIR/name` (empty string when absent).
    pub fn read(&self, name: &str) -> String {
        fs::read_to_string(self.dir().join(name)).unwrap_or_default()
    }

    /// Replace `jobs.tsv`.
    pub fn seed_jobs(&self, jobs: &[Job]) {
        let mut s = String::new();
        for j in jobs {
            s.push_str(&j.to_tsv());
            s.push('\n');
        }
        self.write("jobs.tsv", &s);
    }

    /// Current `jobs.tsv` rows, split into columns.
    pub fn jobs(&self) -> Vec<Vec<String>> {
        self.read("jobs.tsv")
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect()
    }

    /// Every shim invocation so far: `[name, arg, arg, …]` per call.
    pub fn calls(&self) -> Vec<Vec<String>> {
        self.read("calls.log")
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect()
    }

    /// Calls to one shim, without the name column.
    pub fn calls_to(&self, name: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|c| c[0] == name)
            .map(|c| c[1..].to_vec())
            .collect()
    }

    /// `PATH` for commands: the shims first, then a bare system path.
    pub fn path(&self) -> String {
        format!("{}:/usr/bin:/bin", shims_dir().display())
    }

    /// Wire `cmd` to this fixture: env cleared, then `PATH`, `HOME`,
    /// `FAKE_SLURM_DIR`, `SINTERACTIVE_CACHE`, `CLAUDE_CONFIG_DIR`, `USER`.
    pub fn wire(&self, cmd: &mut Command) {
        cmd.env_clear()
            .env("PATH", self.path())
            .env("HOME", self.home_dir())
            .env("USER", "tester")
            .env("FAKE_SLURM_DIR", self.dir())
            .env("SINTERACTIVE_CACHE", self.cache_dir())
            .env("CLAUDE_CONFIG_DIR", self.claude_dir())
            .env("SINTERACTIVE_COLOR", "never")
            .current_dir(self.home_dir());
    }

    /// The `sinteractive` binary under test, wired to this fixture.
    pub fn sinteractive(&self) -> Command {
        let mut cmd = Command::cargo_bin("sinteractive").expect("sinteractive binary");
        self.wire(&mut cmd);
        cmd
    }

    /// One of the shims, run directly (for testing the harness itself).
    pub fn shim(&self, name: &str) -> Command {
        let mut cmd = Command::new(shims_dir().join(name));
        self.wire(&mut cmd);
        cmd
    }
}

impl Default for FakeSlurm {
    fn default() -> Self {
        FakeSlurm::new()
    }
}
