//! `sinteractive events` against a seeded `<cache>/<jobid>.events.ndjson`.

mod common;

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::{FakeSlurm, Job};
use predicates::prelude::*;

const JOB: u64 = 147845;

const LINES: [&str; 3] = [
    r#"{"ts":1783152195,"kind":"started","job":147845,"node":"compute20","name":"web"}"#,
    r#"{"ts":1783178000,"kind":"walltime_warn","remaining":1800}"#,
    r#"{"ts":1783179200,"kind":"walltime_red","remaining":600}"#,
];

fn seed(fx: &FakeSlurm, lines: &[&str]) {
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(fx.cache_dir().join(format!("{JOB}.events.ndjson")), text).expect("seed events");
}

fn stdout_of(assert: assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).expect("utf8")
}

#[test]
fn events_prints_the_log_as_ndjson() {
    let fx = FakeSlurm::new();
    seed(&fx, &LINES);
    let out = stdout_of(
        fx.sinteractive()
            .args(["session", "events", &JOB.to_string()])
            .assert()
            .success(),
    );
    let printed: Vec<&str> = out.lines().collect();
    assert_eq!(printed, LINES, "{out}");
    // Each line is one JSON object with ts and kind first.
    for l in printed {
        let v: serde_json::Value = serde_json::from_str(l).expect("json line");
        assert!(v["ts"].is_i64());
        assert!(v["kind"].is_string());
    }
    // A bare job id needs no Slurm.
    assert!(fx.calls().is_empty(), "{:?}", fx.calls());
}

#[test]
fn events_since_filters_by_timestamp() {
    let fx = FakeSlurm::new();
    seed(&fx, &LINES);
    fx.sinteractive()
        .args([
            "session",
            "events",
            &JOB.to_string(),
            "--since",
            "1783152195",
        ])
        .assert()
        .success()
        .stdout(format!("{}\n{}\n", LINES[1], LINES[2]));
    fx.sinteractive()
        .args([
            "session",
            "events",
            &JOB.to_string(),
            "--since",
            "1783179200",
        ])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn events_missing_log_is_empty_and_malformed_lines_are_skipped() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["session", "events", &JOB.to_string()])
        .assert()
        .success()
        .stdout("");
    seed(&fx, &["garbage", LINES[0], "", "{\"kind\":\"no-ts\"}"]);
    fx.sinteractive()
        .args(["session", "events", &JOB.to_string()])
        .assert()
        .success()
        .stdout(format!("{}\n", LINES[0]));
}

#[test]
fn events_resolves_names_and_the_current_session() {
    let fx = FakeSlurm::with_jobs(&[Job::new(JOB, "sinteractive:web")]);
    seed(&fx, &LINES[..1]);
    fx.sinteractive()
        .args(["session", "events", "web"])
        .assert()
        .success()
        .stdout(format!("{}\n", LINES[0]));
    fx.sinteractive()
        .env("SINTERACTIVE_JOB_ID", JOB.to_string())
        .args(["session", "events"])
        .assert()
        .success()
        .stdout(format!("{}\n", LINES[0]));
    fx.sinteractive()
        .args(["session", "events", "nosuch"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "no sinteractive session named 'nosuch'",
        ));
}

#[test]
fn events_without_a_target_needs_a_session() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["session", "events"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "events requires a JOBID or NAME outside a session",
        ));
}

#[test]
fn events_follow_tails_until_ended() {
    let fx = FakeSlurm::new();
    seed(&fx, &LINES[..1]);
    // Plain std Command: assert_cmd waits for exit, and this one must be
    // observed while it is still following.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sinteractive"));
    cmd.args(["session", "events", "--follow", &JOB.to_string()])
        .env_clear()
        .env("PATH", fx.path())
        .env("HOME", fx.home_dir())
        .env("USER", "tester")
        .env("FAKE_SLURM_DIR", fx.dir())
        .env("SINTERACTIVE_CACHE", fx.cache_dir())
        .env("SINTERACTIVE_COLOR", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("spawn events --follow");
    // Still running after a couple of polls with nothing new.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        matches!(child.try_wait(), Ok(None)),
        "follow exited early: {:?}",
        child.try_wait()
    );
    // Append the rest, then the end: it prints them and stops.
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(fx.cache_dir().join(format!("{JOB}.events.ndjson")))
        .expect("open log");
    writeln!(f, "{}", LINES[1]).unwrap();
    writeln!(
        f,
        r#"{{"ts":1783180952,"kind":"ended","job":{JOB},"reason":"walltime"}}"#
    )
    .unwrap();
    drop(f);
    let deadline = Instant::now() + Duration::from_secs(10);
    while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
        panic!("events --follow did not stop after `ended`");
    }
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "{}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert_eq!(lines[0], LINES[0]);
    assert_eq!(lines[1], LINES[1]);
    assert!(lines[2].contains("\"kind\":\"ended\""), "{text}");
}

#[test]
fn events_follow_stops_when_the_state_file_disappears() {
    let fx = FakeSlurm::new();
    seed(&fx, &LINES[..1]);
    let state = fx.cache_dir().join(format!("{JOB}.json"));
    fs::write(&state, "{}").unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sinteractive"));
    cmd.args(["session", "events", "--follow", &JOB.to_string()])
        .env_clear()
        .env("PATH", fx.path())
        .env("HOME", fx.home_dir())
        .env("USER", "tester")
        .env("FAKE_SLURM_DIR", fx.dir())
        .env("SINTERACTIVE_CACHE", fx.cache_dir())
        .env("SINTERACTIVE_COLOR", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("spawn events --follow");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(matches!(child.try_wait(), Ok(None)), "follow exited early");
    fs::remove_file(&state).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
        panic!("events --follow did not stop when the state file went away");
    }
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{}\n", LINES[0])
    );
}
