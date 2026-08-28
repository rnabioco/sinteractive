//! The launch flow against the fake Slurm: submission, defaults, the
//! maintenance fit, the job-limit refusal, and the detached report.

mod common;

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use common::{FakeSlurm, Job};
use predicates::prelude::*;

/// Where the readiness marker goes (`SINTERACTIVE_RUNTIME_DIR`).
fn runtime_dir(fx: &FakeSlurm) -> PathBuf {
    fx.tmp.path().join("runtime")
}

/// Create the marker `__job` would leave once the session is up.
fn mark_ready(fx: &FakeSlurm, job_id: u64) {
    let dir = runtime_dir(fx).join(format!("sint-{job_id}"));
    fs::create_dir_all(&dir).expect("runtime dir");
    fs::write(dir.join("ready"), "").expect("ready marker");
}

/// The binary wired for a launch: no waits, a private runtime dir, UTC so
/// reservation timestamps round-trip predictably.
fn launcher(fx: &FakeSlurm) -> Command {
    let mut cmd = fx.sinteractive();
    cmd.env("SINTERACTIVE_POLL_FAST", "0")
        .env("SINTERACTIVE_RUNTIME_DIR", runtime_dir(fx))
        .env("TZ", "UTC");
    cmd
}

/// The sbatch options of the one submission.
fn sbatch_options(fx: &FakeSlurm) -> Vec<String> {
    let calls = fx.calls_to("sbatch");
    assert_eq!(calls.len(), 1, "one sbatch call: {calls:?}");
    calls[0].clone()
}

/// `sbatch.last`: the script and its arguments, one per line.
fn script_args(fx: &FakeSlurm) -> Vec<String> {
    fx.read("sbatch.last").lines().map(str::to_string).collect()
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// `YYYY-MM-DDTHH:MM:SS` in UTC, the way the binary reads it under `TZ=UTC`.
fn slurm_stamp(epoch: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(epoch)
        .unwrap()
        .format(&time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]"
        ))
        .unwrap()
}

#[test]
fn detach_json_submits_with_defaults_and_reports() {
    let fx = FakeSlurm::new();
    mark_ready(&fx, 1000);
    let out = launcher(&fx)
        .args(["--detach", "--json", "-n", "web", "--gres=gpu:1"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Submitted job 1000, waiting for it to start.")
                .and(predicate::str::contains("Allocated fakenode01"))
                .and(predicate::str::contains(
                    "Session 1000 is ready on fakenode01.",
                ))
                .and(predicate::str::contains(
                    "Attach:   sinteractive attach web",
                ))
                .and(predicate::str::contains(
                    "Status:   sinteractive status web",
                ))
                .and(predicate::str::contains(
                    "Cancel:   sinteractive cancel web",
                )),
        )
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&out).expect("one JSON object");
    assert_eq!(json["job_id"], 1000);
    assert_eq!(json["name"], "web");
    assert_eq!(json["state"], "RUNNING");
    assert_eq!(json["node"], "fakenode01");
    assert_eq!(json["partition"], "interactive");
    assert_eq!(json["cpus"], 2);
    assert_eq!(json["memory"], "8G");
    assert_eq!(json["memory_mb"], 8192);
    assert_eq!(json["gpus"], 1);
    assert_eq!(json["time_limit"], "24:00:00");
    assert!(json.get("created").is_none(), "created is ensure's: {json}");
    assert!(json.get("cwd").is_none(), "cwd is list's: {json}");

    let opts = sbatch_options(&fx);
    assert_eq!(opts[0], "--output=/dev/null");
    assert_eq!(opts[1], "--error=/dev/null");
    for want in [
        "--time=24:00:00",
        "--partition=interactive",
        "--cpus-per-task=2",
        "--mem=8G",
        "--job-name=sint-web",
        "--gres=gpu:1",
    ] {
        assert!(opts.iter().any(|o| o == want), "{want} in {opts:?}");
    }
    assert!(
        !opts.iter().any(|o| o.starts_with("--qos")),
        "no --qos without SINTERACTIVE_QOS: {opts:?}"
    );
    // Defaults come first so an explicit flag wins with sbatch.
    let gres = opts.iter().position(|o| o == "--gres=gpu:1").unwrap();
    let time = opts.iter().position(|o| o == "--time=24:00:00").unwrap();
    assert!(time < gres);

    let script = script_args(&fx);
    assert_eq!(
        script[0], "exec",
        "the job is `exec EXE __job …`: {script:?}"
    );
    assert!(
        script[1].ends_with("sinteractive"),
        "the running binary is exec'd: {script:?}"
    );
    assert_eq!(script[2], "__job");
    assert!(script.contains(&"--mouse".to_string()), "{script:?}");
    assert!(
        script.contains(&"--session-name=web".to_string()),
        "{script:?}"
    );
    assert!(
        !script.iter().any(|a| a.starts_with("--maint")),
        "{script:?}"
    );

    // Comment tagged via scontrol, and visible in the queue.
    assert!(fx.calls_to("scontrol").contains(&vec![
        "update".to_string(),
        "JobId=1000".to_string(),
        "Comment=sinteractive:web".to_string()
    ]));
    assert_eq!(fx.jobs()[0][1], "sinteractive:web");

    // One ssh to the node, probing for the readiness marker.
    let ssh = fx.calls_to("ssh");
    assert_eq!(ssh.len(), 1, "{ssh:?}");
    assert!(ssh[0].contains(&"fakenode01".to_string()), "{ssh:?}");
    let probe = ssh[0].last().unwrap();
    assert!(probe.contains("sint-1000/ready"), "{probe}");
}

#[test]
fn explicit_flags_and_env_defaults_win_over_builtins() {
    let fx = FakeSlurm::new();
    mark_ready(&fx, 1000);
    launcher(&fx)
        .env("SINTERACTIVE_QOS", "cpu-normal")
        .args([
            "--detach",
            "--no-mouse",
            "-t",
            "2h",
            "-j",
            "4",
            "-m",
            "16G",
            "--partition",
            "rna",
            "--node",
            "n01",
        ])
        .assert()
        .success();
    let opts = sbatch_options(&fx);
    for want in [
        "--time=02:00:00",
        "--cpus-per-task=4",
        "--mem=16G",
        "--partition=rna",
        "--nodelist=n01",
        "--qos=cpu-normal",
    ] {
        assert!(opts.iter().any(|o| o == want), "{want} in {opts:?}");
    }
    assert!(
        !opts
            .iter()
            .any(|o| o == "--time=24:00:00" || o == "--mem=8G"),
        "no overridden defaults: {opts:?}"
    );
    assert!(
        !opts.iter().any(|o| o.starts_with("--job-name")),
        "unnamed: {opts:?}"
    );
    let script = script_args(&fx);
    assert_eq!(script[2..], ["__job".to_string()], "{script:?}");
    assert_eq!(fx.jobs()[0][1], "sinteractive");
}

#[test]
fn sbatch_failure_quotes_stderr_and_the_passthrough() {
    let fx = FakeSlurm::new();
    fx.write(
        "sbatch.fail",
        "sbatch: error: Batch job submission failed: Invalid partition name specified\n",
    );
    launcher(&fx)
        .args(["--detach", "--gres=gpu:1", "--exclusive"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("job submission failed. sbatch said:")
                .and(predicate::str::contains(
                    "  sbatch: error: Batch job submission failed: Invalid partition name specified",
                ))
                .and(predicate::str::contains(
                    "passed through to sbatch: --gres=gpu:1 --exclusive",
                )),
        );
    assert!(fx.jobs().is_empty());
    // The reservation query runs before sbatch; nothing is tagged after it.
    assert!(!fx
        .calls_to("scontrol")
        .iter()
        .any(|c| c.first().is_some_and(|a| a == "update")));
}

#[test]
fn maintenance_overlap_trims_time_and_carries_the_window() {
    let fx = FakeSlurm::new();
    mark_ready(&fx, 1000);
    let start = now_epoch() + 2 * 3600;
    fx.write(
        "reservations",
        &format!(
            "ReservationName=maint StartTime={} EndTime={} Flags=MAINT,IGNORE_JOBS Nodes=ALL\n",
            slurm_stamp(start),
            slurm_stamp(start + 12 * 3600)
        ),
    );
    launcher(&fx).args(["--detach"]).assert().success().stderr(
        predicate::str::contains("Maintenance (maint) starts").and(predicate::str::contains(
            "Shortened the request from 24:00:00 to 01:5",
        )),
    );
    let opts = sbatch_options(&fx);
    let time = opts
        .iter()
        .find(|o| o.starts_with("--time="))
        .expect("a --time");
    assert!(
        time.starts_with("--time=01:5"),
        "trimmed to the gap: {time}"
    );
    let script = script_args(&fx);
    let maint = script
        .iter()
        .find(|a| a.starts_with("--maint="))
        .expect("carried into __job");
    let (name, epoch) = maint["--maint=".len()..].split_once('@').unwrap();
    assert_eq!(name, "maint");
    let epoch: i64 = epoch.parse().unwrap();
    // Ends MAINT_MARGIN before the window, give or take the test's own clock.
    assert!((start - 300 - epoch).abs() <= 5, "{maint} vs start {start}");
}

#[test]
fn maintenance_too_close_refuses() {
    let fx = FakeSlurm::new();
    let start = now_epoch() + 5 * 60;
    fx.write(
        "reservations",
        &format!(
            "ReservationName=maint StartTime={} EndTime={} Flags=MAINT\n",
            slurm_stamp(start),
            slurm_stamp(start + 3600)
        ),
    );
    launcher(&fx).args(["--detach"]).assert().code(1).stderr(
        predicate::str::contains("maintenance starts too soon to open a session")
            .and(predicate::str::contains("Reservation:  maint")),
    );
    assert!(fx.calls_to("sbatch").is_empty());
    // An explicit --reservation skips the check.
    mark_ready(&fx, 1000);
    launcher(&fx)
        .args(["--detach", "--reservation=maint"])
        .assert()
        .success();
}

#[test]
fn job_limit_refuses_with_the_table() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    fx.write("qos", "1\n");
    launcher(&fx).args(["--detach"]).assert().code(1).stderr(
        predicate::str::contains("You already have 1/1 interactive jobs")
            .and(predicate::str::contains("Your current interactive jobs:"))
            .and(predicate::str::contains("JOBID"))
            .and(predicate::str::contains("147845"))
            .and(predicate::str::contains("sint-web"))
            .and(predicate::str::contains("node01"))
            .and(predicate::str::contains("sinteractive attach 147845"))
            .and(predicate::str::contains("scancel 147845")),
    );
    assert!(fx.calls_to("sbatch").is_empty());
    // Another partition is not counted against it.
    mark_ready(&fx, 1000);
    launcher(&fx)
        .args(["--detach", "-p", "rna"])
        .assert()
        .success();
}

#[test]
fn existing_session_is_noted_but_not_refused() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    mark_ready(&fx, 1000);
    launcher(&fx)
        .args(["--detach", "-n", "other"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains(
                "Note: you already have a running session named 'web' (job 147845 on node01).",
            )
            .and(predicate::str::contains(
                "Reattach with 'sinteractive attach web'; starting a new session...",
            )),
        );
}

#[test]
fn duplicate_name_is_refused() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    launcher(&fx)
        .args(["--detach", "-n", "web"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("A sinteractive session named 'web' is already running:")
                .and(predicate::str::contains("JobID(s): 147845"))
                .and(predicate::str::contains("sinteractive attach web")),
        );
    assert!(fx.calls_to("sbatch").is_empty());
}

#[test]
fn invalid_name_is_refused() {
    let fx = FakeSlurm::new();
    launcher(&fx)
        .args(["--detach", "-n", "bad name"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "--name must contain only letters, digits, '.', '_', '-'",
        ));
    assert!(fx.calls_to("sbatch").is_empty());
}

#[test]
fn readiness_timeout_leaves_the_job_and_says_how_to_reach_it() {
    let fx = FakeSlurm::new();
    // No marker: the probe gives up after its 30 tries.
    launcher(&fx).args(["--detach"]).assert().code(1).stderr(
        predicate::str::contains("session did not come up on fakenode01 within 30s.")
            .and(predicate::str::contains("sinteractive attach 1000")),
    );
    assert_eq!(fx.jobs().len(), 1, "the job is not cancelled");
    assert!(fx.calls_to("scancel").is_empty());
}

#[test]
fn launch_inside_a_session_only_allows_detach() {
    let fx = FakeSlurm::new();
    launcher(&fx)
        .env("SINTERACTIVE_JOB_ID", "5")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "Already inside an sinteractive session. Exit this session first.",
        ));
    assert!(fx.calls_to("sbatch").is_empty());
    mark_ready(&fx, 1000);
    launcher(&fx)
        .env("SINTERACTIVE_JOB_ID", "5")
        .args(["--detach"])
        .assert()
        .success();
}

#[test]
fn vanished_job_aborts_with_scancel() {
    let fx = FakeSlurm::new();
    // The job the shim hands out is 1000; make its very first squeue
    // report a terminal state by pre-seeding a COMPLETING row with that id
    // and pointing next_id at it.
    fx.seed_jobs(&[Job::new(1000, "").state("COMPLETING")]);
    fx.write("next_id", "1000\n");
    launcher(&fx)
        .args(["--detach"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "job is neither RUNNING nor PENDING. Aborting.",
        ));
    assert_eq!(fx.calls_to("scancel"), vec![vec!["--quiet", "1000"]]);
}
