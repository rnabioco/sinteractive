//! `queue` against the fake Slurm: one running session, one pending job,
//! and a day of `sacct` history.

mod common;

use common::{FakeSlurm, Job};
use predicates::prelude::*;
use serde_json::Value;

const SACCT: &str = "\
31757001|sint-mywork|interactive|COMPLETED|08:00:04|32G|1234K|2|2026-08-27T22:00:04
31756990|bwa-align|rna|FAILED|00:00:03|64G|40G|16|2026-08-27T13:00:03
31757353|cargo-ci|rna|RUNNING|00:19:01|32G||8|Unknown
";

fn fixture() -> FakeSlurm {
    let fx = FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web").tres("gres:gpu:1"),
        Job::new(147900, "")
            .state("PENDING")
            .node("")
            .reason("Resources")
            .partition("rna")
            .end_time("N/A")
            .elapsed("0:00"),
    ]);
    fx.write("sacct", SACCT);
    fx
}

#[test]
fn queue_json_shape() {
    let fx = fixture();
    let out = fx
        .sinteractive()
        .args(["queue", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).expect("one JSON object");

    let running = v["running"].as_array().unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0]["job_id"], 147845);
    assert_eq!(running[0]["name"], "web");
    assert_eq!(running[0]["job_name"], "sint-web");
    assert_eq!(running[0]["state"], "RUNNING");
    assert_eq!(running[0]["node"], "node01");
    assert_eq!(running[0]["gpus"], 1);
    assert!(running[0].get("reason").is_none());

    let pending = v["pending"].as_array().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["job_id"], 147900);
    assert_eq!(pending[0]["state"], "PENDING");
    assert_eq!(pending[0]["reason"], "Resources");
    assert_eq!(pending[0]["partition"], "rna");
    assert_eq!(pending[0]["node"], Value::Null);

    // Only finished jobs, newest first; the running sacct row is not history.
    let recent = v["recent"].as_array().unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0]["job_id"], "31757001");
    assert_eq!(recent[0]["state"], "COMPLETED");
    assert_eq!(recent[0]["req_mem"], "32G");
    assert_eq!(recent[0]["max_rss"], "1234K");
    assert_eq!(recent[1]["job_id"], "31756990");
    assert_eq!(recent[1]["state"], "FAILED");

    assert_eq!(v["partitions"], serde_json::json!([]));

    let calls = fx.calls_to("sacct");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].contains(&"now-1day".to_string()), "{calls:?}");
}

#[test]
fn queue_all_summarises_partitions() {
    let fx = fixture();
    let out = fx
        .sinteractive()
        .args(["queue", "--all", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v["partitions"],
        serde_json::json!([
            {"partition": "interactive", "running": 1, "pending": 0},
            {"partition": "rna", "running": 0, "pending": 1}
        ])
    );
    assert!(fx
        .calls_to("squeue")
        .iter()
        .any(|c| c == &["-h", "-o", "%P|%T"]));
}

#[test]
fn queue_human_tables_and_memory_hint() {
    let fx = fixture();
    let out = String::from_utf8(
        fx.sinteractive()
            .arg("queue")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    assert!(out.starts_with("Running (1)\n"), "{out}");
    // Column padding is not the contract; the words and their order are.
    let squashed = squash(&out);
    for needle in [
        "JOBID NAME PARTITION NODE ELAPSED/LIMIT CPUS GPUS\n",
        "147845 sint-web interactive node01 1:02:03/8:00:00 4 1\n",
        "\nPending (1)\n",
        "JOBID NAME PARTITION REASON EST. START\n",
        "147900 sinteractive rna waiting for free resources\n",
        "\nRecent (last 24 h)\n",
        "31757001 sint-mywork COMPLETED 08:00:04 1M of 32G ↓ could use 256M\n",
        "31756990 bwa-align FAILED 00:00:03 40G of 64G\n",
    ] {
        assert!(squashed.contains(needle), "missing {needle:?} in:\n{out}");
    }
    assert!(!out.contains("cargo-ci"), "{out}");
    assert!(!out.contains("Partitions"), "{out}");

    let out = String::from_utf8(
        fx.sinteractive()
            .args(["queue", "--all"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    assert!(out.contains("\nPartitions (everyone's jobs)\n"), "{out}");
    let squashed = squash(&out);
    assert!(squashed.contains("interactive 1 0\n"), "{out}");
    assert!(squashed.contains("rna 0 1\n"), "{out}");
}

/// Each line with runs of spaces collapsed and ends trimmed.
fn squash(s: &str) -> String {
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|l| format!("{l}\n"))
        .collect()
}

#[test]
fn queue_with_nothing_to_show() {
    let fx = FakeSlurm::new();
    fx.sinteractive().arg("queue").assert().success().stdout(
        predicate::str::contains("no running jobs")
            .and(predicate::str::contains("no pending jobs"))
            .and(predicate::str::contains("no finished jobs")),
    );
    let out = fx
        .sinteractive()
        .args(["queue", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"running": [], "pending": [], "recent": [], "partitions": []})
    );
}
