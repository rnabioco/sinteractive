//! `attach [TARGET]` against the fake Slurm. The shims exec the attach
//! command locally, which reaches the not-yet-implemented `__attach` verb
//! and fails — the assertions are on `calls.log`, not the exit status.

mod common;

use common::{FakeSlurm, Job};
use predicates::prelude::*;

#[test]
fn attach_with_no_sessions_says_how_to_start_one() {
    let fx = FakeSlurm::new();
    fx.sinteractive().arg("attach").assert().code(1).stderr(
        predicate::str::contains("no running sinteractive sessions to attach to.")
            .and(predicate::str::contains("Start one with 'sinteractive'.")),
    );
    assert!(fx.calls_to("srun").is_empty());
}

#[test]
fn bare_attach_with_several_sessions_lists_them() {
    let fx = FakeSlurm::with_jobs(&[
        Job::default(),
        Job::new(147846, "sinteractive")
            .node("node02")
            .elapsed("5:00"),
        // A pending session is not something to attach to.
        Job::new(147850, "sinteractive:queued").state("PENDING"),
    ]);
    fx.sinteractive().arg("attach").assert().code(1).stderr(
        predicate::str::contains("you have 2 running sessions — pick one:")
            .and(predicate::str::contains("sinteractive attach web"))
            .and(predicate::str::contains(
                "# job 147845 on node01, up 1:02:03",
            ))
            .and(predicate::str::contains("sinteractive attach 147846"))
            .and(predicate::str::contains("# job 147846 on node02, up 5:00"))
            .and(predicate::str::contains("queued").not()),
    );
    assert!(fx.calls_to("srun").is_empty());
}

#[test]
fn bare_attach_with_one_session_goes_straight_to_it() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    fx.sinteractive()
        .arg("attach")
        .assert()
        .stderr(predicate::str::contains(
            "Reattaching to sinteractive session 147845...",
        ));
    let srun = fx.calls_to("srun");
    assert_eq!(srun.len(), 1, "{srun:?}");
    assert_eq!(srun[0][..3], ["--overlap", "--jobid=147845", "--pty"]);
}

#[test]
fn attach_by_id_execs_srun_overlap() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    let _ = fx.sinteractive().args(["attach", "147845"]).assert();
    let srun = fx.calls_to("srun");
    assert_eq!(srun.len(), 1, "{srun:?}");
    let call = &srun[0];
    assert_eq!(call[..3], ["--overlap", "--jobid=147845", "--pty"]);
    assert!(call[3].ends_with("sinteractive"), "{call:?}");
    assert_eq!(call[4..], ["__attach", "sinteractive-147845"]);
    assert!(fx.calls_to("ssh").is_empty());
}

#[test]
fn attach_by_name_resolves_the_comment() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    let _ = fx.sinteractive().args(["attach", "web"]).assert();
    let srun = fx.calls_to("srun");
    assert_eq!(srun.len(), 1, "{srun:?}");
    assert_eq!(srun[0][1], "--jobid=147845");

    fx.sinteractive()
        .args(["attach", "nope"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "no sinteractive session named 'nope'",
        ));
}

#[test]
fn attach_ssh_goes_through_the_node() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    let _ = fx
        .sinteractive()
        .args(["attach", "--ssh", "147845"])
        .assert();
    let ssh = fx.calls_to("ssh");
    assert_eq!(ssh.len(), 1, "{ssh:?}");
    let call = &ssh[0];
    assert_eq!(call[..3], ["-X", "-t", "node01"]);
    assert!(call[3].ends_with("sinteractive"), "{call:?}");
    assert_eq!(call[4..], ["__attach", "sinteractive-147845"]);
    assert!(fx.calls_to("srun").is_empty());
}

#[test]
fn attach_refuses_a_job_that_is_not_running() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147850, "sinteractive:queued").state("PENDING")]);
    fx.sinteractive()
        .args(["attach", "queued"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "job 147850 is not running (state: PENDING)",
        ));
    fx.sinteractive()
        .args(["attach", "999"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "job 999 is not running (state: unknown)",
        ));
    assert!(fx.calls_to("srun").is_empty());
}

#[test]
fn attach_inside_a_session_is_refused() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    fx.sinteractive()
        .env("SINTERACTIVE_JOB_ID", "147845")
        .args(["attach", "147845"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "Already inside an sinteractive session. Exit this session first.",
        ));
}

#[test]
fn compat_attach_flag_maps_to_the_subcommand() {
    let fx = FakeSlurm::with_jobs(&[Job::default()]);
    fx.sinteractive()
        .args(["--attach", "web"])
        .assert()
        .stderr(predicate::str::contains("deprecated"));
    assert_eq!(fx.calls_to("srun").len(), 1);
}
