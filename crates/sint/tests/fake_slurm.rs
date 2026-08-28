//! Smoke tests for the `tests/fake-slurm/` harness itself, independent of
//! any `sinteractive` code: the shims must replay the fixture faithfully
//! before anything can be asserted through them.

mod common;

use common::{FakeSlurm, Job};
use predicates::prelude::*;

const ROW_FORMAT: &str = "%i|%k|%N|%P|%M|%l|%e|%C|%m|%b|%T|%r|%S";

fn two_jobs() -> FakeSlurm {
    FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web").tres("gres:gpu:1"),
        Job::new(147900, "")
            .node("")
            .state("PENDING")
            .reason("Priority")
            .mem("8G")
            .cpus(2)
            .end_time("N/A")
            .elapsed("0:00"),
    ])
}

#[test]
fn squeue_formats_the_row_contract() {
    let fx = two_jobs();
    let out = fx
        .shim("squeue")
        .args([
            "--me",
            "--states",
            "RUNNING,PENDING",
            "--noheader",
            "-o",
            ROW_FORMAT,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert_eq!(
        out,
        "147845|sinteractive:web|node01|interactive|1:02:03|8:00:00|2026-01-01T08:00:00|4|16G|gres:gpu:1|RUNNING|None|2026-01-01T00:00:00\n\
         147900|(null)||interactive|0:00|8:00:00|N/A|2|8G|N/A|PENDING|Priority|2026-01-01T00:00:00\n"
    );
}

#[test]
fn squeue_filters_by_state_job_and_partition() {
    let fx = two_jobs();
    fx.shim("squeue")
        .args([
            "--me",
            "--states",
            "RUNNING",
            "--noheader",
            "-o",
            "%i|%k|%N",
        ])
        .assert()
        .success()
        .stdout("147845|sinteractive:web|node01\n");
    fx.shim("squeue")
        .args(["--jobs", "147900", "--noheader", "-o", "%T|%r|%S"])
        .assert()
        .success()
        .stdout("PENDING|Priority|2026-01-01T00:00:00\n");
    fx.shim("squeue")
        .args(["--jobs", "1", "--noheader", "-o", "%i"])
        .assert()
        .success()
        .stdout("");
    fx.shim("squeue")
        .args([
            "--me",
            "--partition=interactive",
            "--states=RUNNING,PENDING",
            "--noheader",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("147845").and(predicate::str::contains("147900")));
    fx.shim("squeue")
        .args(["--me", "--partition=gpu", "--noheader", "-o", "%i"])
        .assert()
        .success()
        .stdout("");
    // Width modifiers are tolerated; the header appears without --noheader.
    fx.shim("squeue")
        .args(["--me", "-o", "%.10i %k"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "JOBID COMMENT\n147845 sinteractive:web\n",
        ));
}

#[test]
fn squeue_format_columns_are_padded_like_the_real_one() {
    let fx = two_jobs();
    fx.shim("squeue")
        .args(["--jobs", "147845", "--noheader", "--Format", "batchhost"])
        .assert()
        .success()
        .stdout("node01               \n");
    fx.shim("squeue")
        .args([
            "--jobs",
            "147845",
            "--states",
            "RUNNING",
            "--noheader",
            "--Format",
            "state",
        ])
        .assert()
        .success()
        .stdout("RUNNING              \n");
    fx.shim("squeue")
        .args(["--jobs", "147845", "--noheader", "--Format", "EndTime"])
        .assert()
        .success()
        .stdout("2026-01-01T08:00:00  \n");
    fx.shim("squeue")
        .args(["--me", "--states=RUNNING", "--noheader", "--Format=JobID"])
        .assert()
        .success()
        .stdout("147845               \n");
}

#[test]
fn squeue_rejects_what_it_does_not_model() {
    let fx = two_jobs();
    fx.shim("squeue")
        .args(["--me", "--noheader", "-o", "%Z"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported format code %Z"));
    fx.shim("squeue")
        .args(["--bogus"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported option '--bogus'"));
}

#[test]
fn sbatch_appends_a_running_job_and_scontrol_scancel_edit_it() {
    let fx = two_jobs();
    fx.shim("sbatch")
        .args([
            "--output=/dev/null",
            "--partition=amilan",
            "--time=02:00:00",
            "--cpus-per-task=8",
            "--mem=32G",
            "--gres=gpu:2",
            "/path/to/sinteractive",
            "__job",
            "--session-name",
            "web",
        ])
        .assert()
        .success()
        .stdout("Submitted batch job 1000\n");
    assert_eq!(fx.read("next_id").trim(), "1001");
    assert_eq!(
        fx.read("sbatch.last"),
        "/path/to/sinteractive\n__job\n--session-name\nweb\n"
    );
    fx.shim("squeue")
        .args(["--jobs", "1000", "--noheader", "-o", ROW_FORMAT])
        .assert()
        .success()
        .stdout(
            "1000|(null)|fakenode01|amilan|0:01|02:00:00|2026-01-01T08:00:00|8|32G|gres:gpu:2|RUNNING|None|2026-01-01T00:00:00\n",
        );

    fx.shim("scontrol")
        .args(["update", "JobId=1000", "Comment=sinteractive:new"])
        .assert()
        .success();
    fx.shim("scontrol")
        .args(["update", "JobId=1000", "Name=sint-new"])
        .assert()
        .success();
    fx.shim("squeue")
        .args(["--jobs", "1000", "--noheader", "-o", "%k|%j"])
        .assert()
        .success()
        .stdout("sinteractive:new|sint-new\n");
    fx.shim("scontrol")
        .args(["update", "JobId=99", "Comment=x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid job id"));

    fx.shim("scancel")
        .args(["--quiet", "1000"])
        .assert()
        .success();
    assert_eq!(fx.jobs().len(), 2);
    fx.shim("scancel")
        .args(["1000"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid job id specified"));

    let sbatch_calls = fx.calls_to("sbatch");
    assert_eq!(sbatch_calls.len(), 1);
    assert_eq!(sbatch_calls[0][1], "--partition=amilan");
    assert_eq!(
        fx.calls_to("scancel"),
        vec![vec!["--quiet", "1000"], vec!["1000"]]
    );
}

#[test]
fn sbatch_fail_fixture_fails_the_submit() {
    let fx = FakeSlurm::new();
    fx.write(
        "sbatch.fail",
        "sbatch: error: Batch job submission failed: Invalid partition name specified\n",
    );
    fx.shim("sbatch")
        .args(["--partition=nope", "script"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("Invalid partition name"));
    assert!(fx.jobs().is_empty());
}

#[test]
fn scontrol_show_reservation_and_config() {
    let fx = FakeSlurm::new();
    fx.shim("scontrol")
        .args(["show", "reservation", "-o"])
        .assert()
        .success()
        .stdout("");
    fx.write(
        "reservations",
        "ReservationName=maint StartTime=2026-01-02T06:00:00 EndTime=2026-01-02T18:00:00 Flags=MAINT\n",
    );
    fx.shim("scontrol")
        .args(["show", "reservation", "-o"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ReservationName=maint"));
    fx.shim("scontrol")
        .args(["show", "config"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ClusterName = fake"));
}

#[test]
fn canned_outputs_replay_fixture_files() {
    let fx = FakeSlurm::new();
    fx.shim("sacct")
        .args(["-X", "--parsable2"])
        .assert()
        .success()
        .stdout("");
    fx.write("sacct", "1|done|COMPLETED\n");
    fx.write("qos", "5\n");
    fx.write("sinfo", "interactive up 1\n");
    fx.shim("sacct")
        .args(["-X"])
        .assert()
        .success()
        .stdout("1|done|COMPLETED\n");
    fx.shim("sacctmgr")
        .args([
            "show",
            "qos",
            "interactive",
            "format=MaxJobsPerUser",
            "--noheader",
            "--parsable2",
        ])
        .assert()
        .success()
        .stdout("5\n");
    fx.shim("sinfo")
        .args(["-hN", "-o", "%N"])
        .assert()
        .success()
        .stdout("interactive up 1\n");
}

#[test]
fn srun_and_ssh_run_locally() {
    let fx = FakeSlurm::new();
    fx.shim("srun")
        .args([
            "--overlap",
            "--jobid",
            "147845",
            "-w",
            "node01",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout("hi\n");
    fx.shim("srun")
        .args(["--overlap", "--jobid=147845", "echo", "there"])
        .assert()
        .success()
        .stdout("there\n");
    fx.shim("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-q",
            "node01",
            "echo",
            "$FAKE_SSH_HOST",
            "&&",
            "echo",
            "$((1 + 1))",
        ])
        .assert()
        .success()
        .stdout("node01\n2\n");
    fx.shim("ssh")
        .args(["node01"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not supported"));
    assert_eq!(fx.calls_to("srun").len(), 2);
    assert_eq!(fx.calls_to("ssh").len(), 2);
}
