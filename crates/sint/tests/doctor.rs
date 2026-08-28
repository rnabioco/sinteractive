//! `doctor` against the fake Slurm: the shims stand in for the client tools
//! and `ssh`, so the local checks pass, and an empty `PATH` makes the Slurm
//! check fail.

mod common;

use std::fs;

use common::FakeSlurm;
use predicates::prelude::*;
use serde_json::Value;

fn checks(report: &Value) -> Vec<(&str, &str, &str)> {
    report["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap(),
                c["status"].as_str().unwrap(),
                c["detail"].as_str().unwrap(),
            )
        })
        .collect()
}

fn status_of<'a>(checks: &[(&'a str, &'a str, &'a str)], name: &str) -> (&'a str, &'a str) {
    checks
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, s, d)| (*s, *d))
        .unwrap_or_else(|| panic!("no check named {name} in {checks:?}"))
}

/// A build with `SINT_SKIP_BUNDLE` has no plugin, which doctor rightly
/// reports as a failure; the exit-code assertions allow for that.
fn plugin_embedded() -> bool {
    option_env!("SINT_SKIP_BUNDLE").is_none()
}

#[test]
fn doctor_json_reports_a_healthy_install() {
    let fx = FakeSlurm::new();
    let assert = fx.sinteractive().args(["doctor", "--json"]).assert();
    let assert = if plugin_embedded() {
        assert.success()
    } else {
        assert.code(1)
    };
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let report: Value = serde_json::from_str(out.trim()).expect("one JSON object");
    let checks = checks(&report);

    let (status, detail) = status_of(&checks, "binary");
    assert_eq!(status, "ok");
    assert!(
        detail.contains("sinteractive") && detail.contains("zellij "),
        "{detail}"
    );
    assert_eq!(
        status_of(&checks, "slurm"),
        ("ok", "squeue sbatch scontrol sacct sinfo")
    );
    assert_eq!(status_of(&checks, "cluster"), ("ok", "fake"));
    assert_eq!(status_of(&checks, "ssh").0, "ok");
    // The fixture clears SHELL and no GPU driver is expected here.
    assert_eq!(status_of(&checks, "shell").0, "warn");
    assert!(matches!(status_of(&checks, "nvml").0, "ok" | "warn"));
    assert_eq!(
        status_of(&checks, "home"),
        ("ok", "cache dir set by SINTERACTIVE_CACHE")
    );
    let (status, detail) = status_of(&checks, "bundle");
    assert_eq!(status, "ok");
    assert!(
        detail.contains(&fx.cache_dir().join("bin").display().to_string()),
        "{detail}"
    );
    assert!(fx.cache_dir().join("bin").is_dir());
    assert_eq!(status_of(&checks, "cache").0, "ok");
    assert_eq!(
        status_of(&checks, "plugin").0,
        if plugin_embedded() { "ok" } else { "fail" }
    );
    assert_eq!(report["nodes"], serde_json::json!([]));
}

#[test]
fn doctor_human_output_lists_every_check() {
    let fx = FakeSlurm::new();
    let assert = fx.sinteractive().arg("doctor").assert();
    let assert = if plugin_embedded() {
        assert.success()
    } else {
        assert.code(1)
    };
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.starts_with("sinteractive doctor\n"), "{out}");
    for name in [
        "binary", "plugin", "bundle", "cache", "slurm", "cluster", "ssh", "shell", "nvml", "home",
    ] {
        assert!(
            out.contains(&format!(" {name} ")) || out.contains(&format!(" {name}  ")),
            "missing {name} in:\n{out}"
        );
    }
    assert!(out.contains("✓ slurm"), "{out}");
    assert!(!out.contains("Nodes"), "{out}");
}

#[test]
fn doctor_fails_without_slurm_on_path() {
    let fx = FakeSlurm::new();
    let empty = fx.tmp.path().join("empty-bin");
    fs::create_dir_all(&empty).unwrap();
    let out = fx
        .sinteractive()
        .env("PATH", &empty)
        .args(["doctor", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&out).unwrap();
    let checks = checks(&report);
    let (status, detail) = status_of(&checks, "slurm");
    assert_eq!(status, "fail");
    assert!(
        detail.contains("squeue") && detail.contains("sinfo"),
        "{detail}"
    );
    assert_eq!(status_of(&checks, "ssh").0, "fail");
    assert_eq!(status_of(&checks, "cluster").0, "warn");

    fx.sinteractive()
        .env("PATH", &empty)
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("✗ slurm"));
}

#[test]
fn doctor_nodes_sweeps_sinfo_over_ssh() {
    let fx = FakeSlurm::new();
    fx.write("sinfo", "node02\nnode01\nnode02\n");
    let assert = fx
        .sinteractive()
        .args(["doctor", "--nodes", "--json"])
        .assert();
    let assert = if plugin_embedded() {
        assert.success()
    } else {
        assert.code(1)
    };
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let report: Value = serde_json::from_str(out.trim()).unwrap();
    let nodes = report["nodes"].as_array().expect("nodes");
    assert_eq!(nodes.len(), 2, "{nodes:?}");
    assert_eq!(nodes[0]["node"], "node01");
    assert_eq!(nodes[1]["node"], "node02");
    // The ssh shim runs the probe locally: this binary answers, and the
    // bundle the local check just extracted is visible.
    for n in nodes {
        assert_eq!(n["reachable"], true, "{n}");
        assert_eq!(n["version"], env!("CARGO_PKG_VERSION"), "{n}");
        assert_eq!(n["bundle"], true, "{n}");
    }
    let calls = fx.calls_to("ssh");
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().any(|c| c.contains(&"node01".to_string())));
    assert!(calls
        .iter()
        .all(|c| c.contains(&"ConnectTimeout=5".to_string())));

    let out = String::from_utf8(
        fx.sinteractive()
            .args(["doctor", "--nodes"])
            .assert()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    assert!(
        out.contains("Nodes (2): 2 reachable, 2 with this version, 2 see the bundle"),
        "{out}"
    );
}
