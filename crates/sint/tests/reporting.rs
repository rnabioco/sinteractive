//! `status`, `refresh`, `list`, `cancel`, `quota` and `agent-context`
//! against the fake-slurm harness.

mod common;

use std::fs;

use common::{FakeSlurm, Job};
use predicates::prelude::*;
use serde_json::Value;
use sint_core::time::slurm_timestamp_to_epoch;

fn stdout_of(assert: assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).expect("utf8")
}

fn json_of(assert: assert_cmd::assert::Assert) -> Value {
    serde_json::from_str(stdout_of(assert).trim()).expect("valid json")
}

/// A running named session plus a pending unnamed one and a non-session job.
fn mixed_queue() -> FakeSlurm {
    FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web").tres("gres:gpu:1"),
        Job::new(147846, "sinteractive")
            .node("")
            .state("PENDING")
            .reason("Priority")
            .end_time("N/A")
            .elapsed("0:00"),
        Job::new(147900, "cargo-ci").node("node02"),
    ])
}

// ---- status -------------------------------------------------------------

#[test]
fn status_json_is_byte_exact_for_a_running_named_session() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web").tres("gres:gpu:1")]);
    let out = stdout_of(
        fx.sinteractive()
            .args(["status", "147845", "--json"])
            .assert()
            .success(),
    );
    let end = slurm_timestamp_to_epoch("2026-01-01T08:00:00").expect("fixture end time");
    // The fixture is in the past, so remaining clamps at zero and the line
    // is fully determined.
    assert_eq!(
        out,
        format!(
            "{{\"job_id\":147845,\"name\":\"web\",\"state\":\"RUNNING\",\"node\":\"node01\",\
             \"partition\":\"interactive\",\"cpus\":4,\"memory\":\"16G\",\"memory_mb\":16384,\
             \"gpus\":1,\"time_limit\":\"8:00:00\",\"elapsed\":\"1:02:03\",\
             \"end_epoch\":{end},\"remaining_seconds\":0}}\n"
        )
    );
}

#[test]
fn status_by_name_and_by_id() {
    let fx = mixed_queue();
    let v = json_of(
        fx.sinteractive()
            .args(["status", "web", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(v["job_id"], 147845);
    assert_eq!(v["name"], "web");
    assert!(v.get("cwd").is_none(), "status never carries cwd");
    assert!(v.get("created").is_none(), "status never carries created");

    // Non-session jobs are still reported by id, with a null name.
    let v = json_of(
        fx.sinteractive()
            .args(["status", "147900", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(v["name"], Value::Null);
    assert_eq!(v["node"], "node02");

    // Pending: no node, no budget.
    let v = json_of(
        fx.sinteractive()
            .args(["status", "147846", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(v["state"], "PENDING");
    assert_eq!(v["node"], Value::Null);
    assert_eq!(v["end_epoch"], Value::Null);
    assert_eq!(v["remaining_seconds"], Value::Null);
}

#[test]
fn status_human_block() {
    let fx = mixed_queue();
    fx.sinteractive()
        .args(["status", "web"])
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("Session 147845 (web): RUNNING on node01\n")
                .and(predicate::str::contains("  Partition:  interactive\n"))
                .and(predicate::str::contains(
                    "  Resources:  4 CPUs, 16G, 1 GPU\n",
                ))
                .and(predicate::str::contains(
                    "  Elapsed:    1:02:03 (limit 8:00:00)\n",
                ))
                .and(predicate::str::contains("  Remaining:  0s\n")),
        );
    fx.sinteractive()
        .args(["status", "147846"])
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("Session 147846: PENDING\n")
                .and(predicate::str::contains("Remaining:").not()),
        );
}

#[test]
fn status_shows_notices_from_the_cache() {
    let fx = mixed_queue();
    fs::write(
        fx.cache_dir().join("147845.notices"),
        "quota\tQUOTA over by 1G (5G limit)\nmaint\tSession ends Thu Sep 3 07:55 — trimmed\n",
    )
    .expect("seed notices");
    fx.sinteractive()
        .args(["status", "web"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("  Notice:     QUOTA over by 1G (5G limit)\n").and(
                predicate::str::contains("  Notice:     Session ends Thu Sep 3 07:55 — trimmed\n"),
            ),
        );
}

#[test]
fn status_unknown_and_ambiguous_names_exit_1() {
    let fx = mixed_queue();
    fx.sinteractive()
        .args(["status", "nope"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(
            predicate::str::contains("no sinteractive session named 'nope'")
                .and(predicate::str::contains("sinteractive list")),
        );
    fx.sinteractive()
        .args(["status", "nope", "--json"])
        .assert()
        .code(1)
        .stdout("");

    let fx = FakeSlurm::with_jobs(&[
        Job::new(10, "sinteractive:dup"),
        Job::new(11, "sinteractive:dup"),
    ]);
    fx.sinteractive()
        .args(["status", "dup"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("multiple sinteractive sessions named 'dup':\n  10 11\n")
                .and(predicate::str::contains("Specify a JOBID instead.")),
        );
}

#[test]
fn status_not_found_json_and_human() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["status", "999", "--json"])
        .assert()
        .code(1)
        .stdout("{\"job_id\":999,\"state\":\"NOT_FOUND\"}\n");
    fx.sinteractive()
        .args(["status", "999"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "job 999 not found (finished or cancelled)",
        ));
}

#[test]
fn status_without_a_target_uses_the_session_env_or_fails() {
    let fx = mixed_queue();
    let v = json_of(
        fx.sinteractive()
            .env("SINTERACTIVE_JOB_ID", "147845")
            .args(["status", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(v["job_id"], 147845);

    fx.sinteractive()
        .arg("status")
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "requires a JOBID or NAME outside a session",
        ));
}

#[test]
fn refresh_pokes_then_reports() {
    let fx = mixed_queue();
    let poke = fx.cache_dir().join("147845.poke");
    assert!(!poke.exists());
    let v = json_of(
        fx.sinteractive()
            .args(["status", "web", "--refresh", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(v["job_id"], 147845);
    assert!(poke.exists(), "refresh touches the poke file");
    // Plain status does not.
    fs::remove_file(&poke).unwrap();
    fx.sinteractive().args(["status", "web"]).assert().success();
    assert!(!poke.exists());
}

#[test]
fn compat_status_flag_is_the_same_json_plus_a_warning() {
    let fx = mixed_queue();
    let new = stdout_of(
        fx.sinteractive()
            .args(["status", "web", "--json"])
            .assert()
            .success(),
    );
    let old = fx
        .sinteractive()
        .args(["--status", "web", "--json"])
        .assert()
        .success()
        .stderr(predicate::str::contains("deprecated"));
    assert_eq!(stdout_of(old), new);
}

// ---- list ---------------------------------------------------------------

#[test]
fn list_json_empty_is_an_empty_array() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout("[]\n");
    fx.sinteractive()
        .arg("list")
        .assert()
        .success()
        .stdout("No running sinteractive sessions.\nStart one with sinteractive.\n");
}

#[test]
fn list_json_carries_cwd_and_skips_pending_and_foreign_jobs() {
    let fx = FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web"),
        Job::new(147846, "sinteractive").node("node03"),
        Job::new(147847, "sinteractive:queued")
            .state("PENDING")
            .node("")
            .end_time("N/A"),
        Job::new(147900, "cargo-ci"),
    ]);
    let out = stdout_of(
        fx.sinteractive()
            .args(["list", "--json"])
            .assert()
            .success(),
    );
    let v: Value = serde_json::from_str(out.trim()).expect("json");
    let rows = v.as_array().expect("array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["job_id"], 147845);
    assert_eq!(rows[0]["name"], "web");
    assert_eq!(rows[1]["job_id"], 147846);
    assert_eq!(rows[1]["name"], Value::Null);
    for r in rows {
        assert_eq!(r["state"], "RUNNING");
        assert!(r.get("cwd").is_some(), "cwd key present: {r}");
        assert_eq!(r["cwd"], Value::Null);
    }
    // cwd is the last key, after remaining_seconds.
    assert!(
        out.contains("\"remaining_seconds\":0,\"cwd\":null}"),
        "{out}"
    );
    assert!(!out.contains("147847"));
    assert!(!out.contains("147900"));
}

#[test]
fn list_human_table() {
    let fx = FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web"),
        Job::new(147846, "sinteractive").node("node03"),
        Job::new(147900, "cargo-ci"),
    ]);
    fx.sinteractive().arg("list").assert().success().stdout(
        predicate::str::contains(
            "JOBID       NAME                  NODE            PARTITION     ELAPSED     TIMELIMIT   CWD\n",
        )
        .and(predicate::str::contains(
            "147845      web                   node01          interactive   1:02:03     8:00:00     -\n",
        ))
        .and(predicate::str::contains(
            "147846      -                     node03          interactive   1:02:03     8:00:00     -\n",
        ))
        .and(predicate::str::contains("Reattach:  sinteractive attach JOBID|NAME\n"))
        .and(predicate::str::contains("Cancel:    sinteractive cancel JOBID|NAME\n"))
        .and(predicate::str::contains("147900").not()),
    );
}

// ---- cancel -------------------------------------------------------------

#[test]
fn cancel_by_name_removes_the_job() {
    let fx = mixed_queue();
    fx.sinteractive()
        .args(["cancel", "web"])
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains(
            "Cancelled session 147845 (web) on node01.",
        ));
    let scancel = fx.calls_to("scancel");
    assert_eq!(scancel, vec![vec!["147845".to_string()]]);
    let ids: Vec<String> = fx.jobs().into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["147846", "147900"]);
}

#[test]
fn cancel_json_shape() {
    let fx = mixed_queue();
    fx.sinteractive()
        .args(["cancel", "147845", "--json"])
        .assert()
        .success()
        .stdout("{\"job_id\":147845,\"cancelled\":true}\n");
    assert_eq!(fx.calls_to("scancel").len(), 1);
}

#[test]
fn cancel_failures_exit_1() {
    let fx = mixed_queue();
    fx.sinteractive()
        .args(["cancel", "nope"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "no sinteractive session named 'nope'",
        ));
    assert!(fx.calls_to("scancel").is_empty());
    fx.sinteractive()
        .args(["cancel", "424242"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("could not cancel job 424242"));
}

// ---- quota --------------------------------------------------------------

#[test]
fn quota_json_without_a_cache_is_an_error() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["quota", "--json"])
        .assert()
        .code(1)
        .stdout("{\"error\":\"quota unavailable\"}\n");
    fx.sinteractive()
        .arg("quota")
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains("could not read quota."));
}

#[test]
fn quota_reports_the_cache() {
    let fx = FakeSlurm::new();
    let over = "{\"user\":\"tester\",\"used_kb\":537185280,\"hard_kb\":524288000,\"over_kb\":12897280,\"pct\":102,\"over\":true,\"checked_epoch\":1783152195}\n";
    fs::write(fx.cache_dir().join("quota.json"), over).unwrap();
    fx.sinteractive()
        .arg("quota")
        .assert()
        .success()
        .stdout("OVER QUOTA: 512.3G of 500G used (102%), over by 12.3G\n");
    fx.sinteractive()
        .args(["quota", "--json"])
        .assert()
        .success()
        .stdout(over);

    fs::write(
        fx.cache_dir().join("quota.json"),
        "{\"user\":\"tester\",\"used_kb\":262144000,\"hard_kb\":524288000,\"over_kb\":0,\"pct\":50,\"over\":false,\"checked_epoch\":1783152195}\n",
    )
    .unwrap();
    fx.sinteractive()
        .arg("quota")
        .assert()
        .success()
        .stdout("Quota OK: 250G of 500G used (50%)\n");
}

#[test]
fn quota_check_without_daemons_is_unavailable() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["quota", "--check", "--json"])
        .env("SINTERACTIVE_QUOTA_FILE", fx.tmp.path().join("absent"))
        .assert()
        .code(1)
        .stdout("{\"error\":\"quota unavailable\"}\n");
    fx.sinteractive()
        .args(["quota", "--check"])
        .env("SINTERACTIVE_QUOTA_FILE", fx.tmp.path().join("absent"))
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("could not read quota.")
                .and(predicate::str::contains("SINTERACTIVE_QUOTA_PORT.")),
        );
}

// ---- agent-context ------------------------------------------------------

#[test]
fn agent_context_outside_a_session_exits_1() {
    let fx = mixed_queue();
    fx.sinteractive()
        .args(["claude", "context"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains("SINTERACTIVE_JOB_ID unset"));
}

#[test]
fn agent_context_briefing() {
    let fx = mixed_queue();
    let out = stdout_of(
        fx.sinteractive()
            .env("SINTERACTIVE_JOB_ID", "147845")
            .args(["claude", "context"])
            .assert()
            .success(),
    );
    assert!(
        out.starts_with(
            "You are inside an sinteractive zellij session on a compute node.\n\
             \x20 job 147845 (web) on node01, partition interactive — 4 CPUs, 16G, 1 GPU\n\
             \x20 walltime 0s remaining (the session self-terminates ~10s before the limit)\n\n\
             This session is an orchestration shell, NOT a compute target."
        ),
        "{out}"
    );
    for needle in [
        "srun -p PART -c N --mem SIZE -t TIME -J NAME --comment=NAME -- CMD",
        "salloc --no-shell -p PART",
        "`sinteractive status --json`",
        "(shown in full by `sinteractive status`)",
        "`sinteractive quota --check`",
        "be changed underneath you.\n\nStorage quota, while exceeded,",
    ] {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }
    assert!(!out.contains("--status"), "{out}");
    assert!(!out.contains("--check-quota"), "{out}");
    assert!(!out.contains("OVER STORAGE QUOTA"), "{out}");

    // With an over-quota cache the block appears between the two paragraphs.
    fs::write(
        fx.cache_dir().join("quota.json"),
        "{\"user\":\"tester\",\"used_kb\":537185280,\"hard_kb\":524288000,\"over_kb\":12897280,\"pct\":102,\"over\":true,\"checked_epoch\":1}\n",
    )
    .unwrap();
    let out = stdout_of(
        fx.sinteractive()
            .env("SINTERACTIVE_JOB_ID", "147845")
            .args(["claude", "context"])
            .assert()
            .success(),
    );
    assert!(
        out.contains(
            "be changed underneath you.\n\nOVER STORAGE QUOTA: 512.3G of 500G used (102%), over by 12.3G.\nStorage quota, while exceeded,"
        ),
        "{out}"
    );
}

#[test]
fn agent_context_for_a_vanished_job_exits_1() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .env("SINTERACTIVE_JOB_ID", "5")
        .args(["claude", "context"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("job 5 not found"));
}
