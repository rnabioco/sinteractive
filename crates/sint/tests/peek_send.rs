//! `peek` and `send` against the fake Slurm: the `ssh` shim runs the
//! remote command locally, so what would reach the node is observable in
//! `calls.log`. No zellij server is running there, so the embedded zellij
//! refuses the action — the expected failure path here, and the same one a
//! user sees when a session's server has gone away.

mod common;

use common::{FakeSlurm, Job};
use predicates::prelude::*;

/// `args` starts with the `session` verb (`peek` / `send`).
fn cmd(fx: &FakeSlurm, args: &[&str]) -> assert_cmd::Command {
    let mut c = fx.sinteractive();
    c.env("SINTERACTIVE_RUNTIME_DIR", fx.tmp.path().join("runtime"))
        .arg("session")
        .args(args);
    c
}

/// The single `ssh` call's arguments: `[..options, node, remote_command]`.
fn ssh_call(fx: &FakeSlurm) -> Vec<String> {
    let calls = fx.calls_to("ssh");
    assert_eq!(calls.len(), 1, "one ssh, got {calls:?}");
    calls.into_iter().next().unwrap()
}

#[test]
fn peek_reads_the_screen_over_ssh() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web")]);
    cmd(&fx, &["peek", "147845", "-n", "20"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "could not read the screen of session 147845 on node01",
        ));
    let call = ssh_call(&fx);
    assert!(call.contains(&"BatchMode=yes".to_string()), "{call:?}");
    assert!(call.contains(&"node01".to_string()), "{call:?}");
    let remote = call.last().unwrap();
    assert!(
        remote.contains("zellij action dump-screen -p terminal_0 --full"),
        "{remote}"
    );
    assert!(remote.starts_with("env ZELLIJ_SOCKET_DIR="), "{remote}");
    assert!(remote.contains("/runtime/sint-147845"), "{remote}");
    assert!(
        remote.contains("ZELLIJ_SESSION_NAME=sinteractive-147845"),
        "{remote}"
    );
    assert!(remote.contains("XDG_CACHE_HOME="), "{remote}");
}

#[test]
fn peek_resolves_by_name_and_refuses_a_pending_session() {
    let fx = FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web"),
        Job::new(147900, "sinteractive:queued")
            .state("PENDING")
            .node("")
            .reason("Priority"),
    ]);
    cmd(&fx, &["peek", "web"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("session 147845 on node01"));
    assert!(ssh_call(&fx).contains(&"node01".to_string()));

    let fx = FakeSlurm::with_jobs(&[Job::new(147900, "sinteractive:queued")
        .state("PENDING")
        .node("")
        .reason("Priority")]);
    cmd(&fx, &["peek", "queued"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "session 147900 is not running (state: PENDING)",
        ));
    assert!(fx.calls_to("ssh").is_empty());

    cmd(&fx, &["peek", "nope"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "no sinteractive session named 'nope'",
        ));
    cmd(&fx, &["peek", "424242"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("job 424242 not found"));
}

#[test]
fn send_types_then_presses_enter() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web")]);
    cmd(&fx, &["send", "web", "echo 'hi there'"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "could not send to session 147845 on node01",
        ));
    let call = ssh_call(&fx);
    assert!(call.contains(&"node01".to_string()), "{call:?}");
    let remote = call.last().unwrap();
    let (chars, enter) = remote.split_once(" && ").expect("two chained actions");
    assert!(
        chars.ends_with("zellij action write-chars -p terminal_0 'echo '\\''hi there'\\'''"),
        "{chars}"
    );
    assert!(
        enter.ends_with("zellij action write -p terminal_0 13"),
        "{enter}"
    );
    assert!(chars.contains("/runtime/sint-147845") && enter.contains("/runtime/sint-147845"));
}

#[test]
fn send_refuses_an_empty_command_and_a_pending_session() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web")]);
    cmd(&fx, &["send", "web", "   "])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("COMMAND is empty"));
    assert!(fx.calls_to("ssh").is_empty());

    let fx = FakeSlurm::with_jobs(&[Job::new(147900, "sinteractive:queued")
        .state("PENDING")
        .node("")]);
    cmd(&fx, &["send", "147900", "ls"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("is not running (state: PENDING)"));
    assert!(fx.calls_to("ssh").is_empty());
}
