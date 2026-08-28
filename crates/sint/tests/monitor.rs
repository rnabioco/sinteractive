//! `snapshot` and `monitor` against the fake-slurm harness.
//!
//! `snapshot` samples this host, so only shape is asserted; `monitor` reads
//! a seeded `<cache>/<jobid>.metrics.json`, and `--live` goes through the
//! fake `ssh`, which runs the remote command locally.

mod common;

use std::fs;

use common::{FakeSlurm, Job};
use predicates::prelude::*;
use serde_json::{json, Value};

fn stdout_of(assert: assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).expect("utf8")
}

fn json_of(assert: assert_cmd::assert::Assert) -> Value {
    serde_json::from_str(stdout_of(assert).trim()).expect("valid json")
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// A minimal snapshot as the in-session sampler would write it.
fn seed_snapshot(fx: &FakeSlurm, job_id: u64, ts: i64) {
    let snap = json!({
        "host": "node01",
        "ts": ts,
        "scope": {"job_id": job_id, "cpus_alloc": 4, "mem_alloc_mb": 16384},
        "cpu": {"pct": 42.0, "ncpu": 64, "load1": 3.0, "load5": 2.0, "load15": 1.0},
        "mem": {"total_mb": 16384, "used_mb": 8192},
        "gpus": [],
        "procs": [{"pid": 4242, "user": "tester", "cpu_pct": 150.0, "rss_mb": 2048,
                   "threads": 8, "state": "R", "command": "python train.py"}],
        "cpu_history": [1, 2, 3]
    });
    fs::write(
        fx.cache_dir().join(format!("{job_id}.metrics.json")),
        format!("{snap}\n"),
    )
    .expect("seed snapshot");
}

// ---- snapshot -----------------------------------------------------------

#[test]
fn snapshot_json_describes_this_host() {
    let fx = FakeSlurm::new();
    let v = json_of(
        fx.sinteractive()
            .args(["monitor", "--once", "--json"])
            .assert()
            .success(),
    );
    assert!(v["host"].as_str().is_some_and(|h| !h.is_empty()), "{v}");
    assert!(v["cpu"]["ncpu"].as_u64().unwrap() > 0, "{v}");
    assert!(v["ts"].as_i64().unwrap() > 0);
    assert!(v["gpus"].is_array());
    assert!(v["procs"].as_array().is_some_and(|p| !p.is_empty()));
    assert_eq!(v["cpu_history"].as_array().unwrap().len(), 2, "two samples");
}

#[test]
fn snapshot_human_dump() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["monitor", "--once"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("  cpu  ")
                .and(predicate::str::contains("  mem  "))
                .and(predicate::str::contains("  gpu  "))
                .and(predicate::str::contains("PID USER")),
        );
}

// ---- monitor ------------------------------------------------------------

#[test]
fn monitor_json_prints_the_cached_snapshot() {
    let fx = FakeSlurm::new();
    seed_snapshot(&fx, 147845, now());
    let v = json_of(
        fx.sinteractive()
            .args(["monitor", "--json", "147845"])
            .assert()
            .success(),
    );
    assert_eq!(v["host"], "node01");
    assert_eq!(v["scope"]["job_id"], 147845);
    assert_eq!(v["procs"][0]["pid"], 4242);
    assert_eq!(v["procs"][0]["state"], "R");
    // No Slurm call was needed to read the cache.
    assert!(fx.calls().is_empty(), "{:?}", fx.calls());
}

#[test]
fn monitor_without_a_snapshot_or_with_a_stale_one_exits_1() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["monitor", "--json", "147845"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "no snapshot yet for job 147845 — the session's sampler writes one every 5 s",
        ));
    seed_snapshot(&fx, 147845, now() - 120);
    fx.sinteractive()
        .args(["monitor", "--json", "147845"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(
            predicate::str::contains("snapshot for job 147845 is 12").and(
                predicate::str::contains("s old — the session's sampler writes one every 5 s"),
            ),
        );
}

#[test]
fn monitor_without_a_target_needs_a_session() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .arg("monitor")
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "monitor requires a JOBID, NAME or hostname outside a session",
        ));
    // Inside a session the env names the job.
    seed_snapshot(&fx, 147845, now());
    let v = json_of(
        fx.sinteractive()
            .env("SINTERACTIVE_JOB_ID", "147845")
            .args(["monitor", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(v["scope"]["job_id"], 147845);
}

#[test]
fn monitor_by_name_reads_that_sessions_cache() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web")]);
    seed_snapshot(&fx, 147845, now());
    let v = json_of(
        fx.sinteractive()
            .args(["monitor", "--json", "web"])
            .assert()
            .success(),
    );
    assert_eq!(v["scope"]["job_id"], 147845);
}

#[test]
fn monitor_human_dump_without_a_tty() {
    let fx = FakeSlurm::new();
    seed_snapshot(&fx, 147845, now());
    fx.sinteractive()
        .args(["monitor", "147845"])
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("node01 · job 147845 · 4 CPUs 16G\n")
                .and(predicate::str::contains(
                    "  cpu    42% of 4 · load 3.0 2.0 1.0 · host 64 CPUs\n",
                ))
                .and(predicate::str::contains(
                    "   4242 tester      150.0   2.0G       -    -  python train.py\n",
                )),
        );
}

#[test]
fn monitor_live_samples_the_sessions_node_over_ssh() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web").node("node01")]);
    let v = json_of(
        fx.sinteractive()
            .args(["monitor", "--json", "--live", "web"])
            .assert()
            .success(),
    );
    // The fake ssh ran `snapshot --json` here, so the host is this machine.
    assert!(v["host"].as_str().is_some_and(|h| !h.is_empty()));
    assert!(v["cpu"]["ncpu"].as_u64().unwrap() > 0);
    let ssh = fx.calls_to("ssh");
    assert_eq!(ssh.len(), 1, "{ssh:?}");
    assert_eq!(&ssh[0][..2], ["-o", "BatchMode=yes"]);
    assert!(ssh[0].contains(&"node01".to_string()), "{ssh:?}");
    assert!(ssh[0].last().unwrap().ends_with(" snapshot --json"));

    // A bare hostname implies --live.
    let v = json_of(
        fx.sinteractive()
            .args(["monitor", "--json", "node07"])
            .assert()
            .success(),
    );
    assert!(v["cpu"]["ncpu"].as_u64().unwrap() > 0);
    let ssh = fx.calls_to("ssh");
    assert_eq!(ssh.len(), 2);
    assert!(ssh[1].contains(&"node07".to_string()), "{ssh:?}");
}

#[test]
fn monitor_live_on_a_pending_or_unknown_job_exits_1() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147846, "sinteractive")
        .node("")
        .state("PENDING")
        .end_time("N/A")]);
    fx.sinteractive()
        .args(["monitor", "--json", "--live", "147846"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("has no node yet (PENDING)"));
    fx.sinteractive()
        .args(["monitor", "--json", "--live", "999"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("job 999 not found"));
}
