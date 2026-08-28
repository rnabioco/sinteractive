//! `ensure NAME`: get-or-create against the fake Slurm.

mod common;

use std::fs;

use common::{FakeSlurm, Job};
use predicates::prelude::*;

fn mark_ready(fx: &FakeSlurm, job_id: u64) {
    let dir = fx.tmp.path().join("runtime").join(format!("sint-{job_id}"));
    fs::create_dir_all(&dir).expect("runtime dir");
    fs::write(dir.join("ready"), "").expect("ready marker");
}

fn ensure(fx: &FakeSlurm, args: &[&str]) -> assert_cmd::Command {
    let mut cmd = fx.sinteractive();
    cmd.env("SINTERACTIVE_POLL_FAST", "0")
        .env("SINTERACTIVE_RUNTIME_DIR", fx.tmp.path().join("runtime"))
        .args(["session", "ensure"])
        .args(args);
    cmd
}

fn json_of(out: &[u8]) -> serde_json::Value {
    serde_json::from_slice(out).expect("one JSON object")
}

#[test]
fn ensure_creates_then_reuses() {
    let fx = FakeSlurm::new();
    mark_ready(&fx, 1000);
    let first = ensure(&fx, &["web", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first = json_of(&first);
    assert_eq!(first["created"], true);
    assert_eq!(first["job_id"], 1000);
    assert_eq!(first["name"], "web");
    assert_eq!(first["state"], "RUNNING");

    let second = ensure(&fx, &["web", "--json"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Reusing").not())
        .get_output()
        .stdout
        .clone();
    let second = json_of(&second);
    assert_eq!(second["created"], false);
    assert_eq!(second["job_id"], 1000);
    assert_eq!(second["name"], "web");
    assert_eq!(fx.calls_to("sbatch").len(), 1, "launched once");

    // Human form of the reuse path.
    ensure(&fx, &["web"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Reusing existing session web."))
        .stdout(
            predicate::str::starts_with("Session 1000 (web): RUNNING on fakenode01\n")
                .and(predicate::str::contains("  Partition:  interactive\n"))
                .and(predicate::str::contains("  Resources:  2 CPUs, 8G\n"))
                .and(predicate::str::contains(
                    "  Elapsed:    0:01 (limit 24:00:00)\n",
                )),
        );
}

#[test]
fn ensure_counts_a_pending_session_as_existing() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147850, "sinteractive:queued")
        .state("PENDING")
        .node("")
        .reason("Priority")]);
    let out = ensure(&fx, &["queued", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = json_of(&out);
    assert_eq!(json["created"], false);
    assert_eq!(json["job_id"], 147850);
    assert_eq!(json["state"], "PENDING");
    assert!(json["node"].is_null());
    assert!(fx.calls_to("sbatch").is_empty());
}

#[test]
fn ensure_passes_launch_flags_through() {
    let fx = FakeSlurm::new();
    mark_ready(&fx, 1000);
    ensure(&fx, &["gpu", "-j", "4", "--gres=gpu:1", "--json"])
        .assert()
        .success();
    let opts = &fx.calls_to("sbatch")[0];
    assert!(opts.contains(&"--cpus-per-task=4".to_string()), "{opts:?}");
    assert!(opts.contains(&"--gres=gpu:1".to_string()), "{opts:?}");
    assert!(
        opts.contains(&"--job-name=sint-gpu".to_string()),
        "{opts:?}"
    );
    assert_eq!(fx.jobs()[0][1], "sinteractive:gpu");
}

#[test]
fn compat_ensure_flag_still_works() {
    let fx = FakeSlurm::new();
    mark_ready(&fx, 1000);
    let out = fx
        .sinteractive()
        .env("SINTERACTIVE_POLL_FAST", "0")
        .env("SINTERACTIVE_RUNTIME_DIR", fx.tmp.path().join("runtime"))
        .args(["--ensure", "web", "--json"])
        .assert()
        .success()
        .stderr(predicate::str::contains("deprecated"))
        .get_output()
        .stdout
        .clone();
    assert_eq!(json_of(&out)["created"], true);
}
