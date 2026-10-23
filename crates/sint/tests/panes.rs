//! Pane-aware `session peek` / `send`, `session panes` and `session exec`
//! against the fake Slurm and the fake zellij (`SINTERACTIVE_ZELLIJ`): the
//! `ssh` shim runs the remote script locally, so what would reach the node
//! is observable in `calls.log`.

mod common;

use common::{shims_dir, FakeSlurm, Job};
use predicates::prelude::*;
use serde_json::Value;

const PANES: &str = r#"[
  {"id":0,"is_plugin":true,"is_focused":false,"is_floating":false,"title":"tab-bar","exited":false,"exit_status":null,"tab_name":"Tab #1"},
  {"id":0,"is_plugin":false,"is_focused":true,"is_floating":false,"title":"Pane #1","exited":false,"exit_status":null,"terminal_command":null,"tab_name":"Tab #1","pane_command":"/bin/bash","pane_cwd":"/work"},
  {"id":3,"is_plugin":false,"is_focused":false,"is_floating":true,"title":"build","exited":true,"exit_status":2,"terminal_command":"bash -lc make","tab_name":"Tab #1"}
]"#;

fn fixture() -> FakeSlurm {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web")]);
    fx.write("panes.json", PANES);
    fx.write("screen.terminal_0", "shell line\n\n");
    fx.write("screen.terminal_3", "make: *** [all] Error 2\n");
    // The session's node-local socket directory, where `exec` stages output.
    std::fs::create_dir_all(fx.tmp.path().join("runtime/sint-147845")).unwrap();
    fx
}

fn cmd(fx: &FakeSlurm, args: &[&str]) -> assert_cmd::Command {
    let mut c = fx.sinteractive();
    c.env("SINTERACTIVE_RUNTIME_DIR", fx.tmp.path().join("runtime"))
        .env("SINTERACTIVE_ZELLIJ", shims_dir().join("zellij"))
        .arg("session")
        .args(args);
    c
}

/// The zellij calls the fake saw, as `ACTION ARGS…` strings.
fn zellij_calls(fx: &FakeSlurm) -> Vec<String> {
    fx.calls_to("zellij")
        .into_iter()
        .map(|c| c.join(" "))
        .collect()
}

#[test]
fn panes_lists_terminal_panes_as_json_and_text() {
    let fx = fixture();
    let out = cmd(&fx, &["panes", "web", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["job_id"], 147845);
    assert_eq!(v["node"], "node01");
    let panes = v["panes"].as_array().unwrap();
    assert_eq!(panes.len(), 2, "plugin panes are left out: {v}");
    assert_eq!(panes[0]["id"], "terminal_0");
    assert_eq!(panes[0]["focused"], true);
    assert_eq!(panes[0]["cwd"], "/work");
    assert_eq!(panes[1]["id"], "terminal_3");
    assert_eq!(panes[1]["title"], "build");
    assert_eq!(panes[1]["command"], "bash -lc make");
    assert_eq!(panes[1]["floating"], true);
    assert_eq!(panes[1]["exited"], true);
    assert_eq!(panes[1]["exit_status"], 2);
    assert_eq!(panes[1]["tab"], "Tab #1");
    assert!(panes[1].get("cwd").is_none());
    assert_eq!(
        zellij_calls(&fx),
        ["action list-panes --json --all"],
        "one list-panes"
    );

    cmd(&fx, &["panes", "web"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "terminal_3\tbuild\tTab #1\tbash -lc make\tfloating,exited 2",
        ))
        .stdout(predicate::str::contains(
            "terminal_0\tPane #1\tTab #1\t/bin/bash\tfocused",
        ));
}

#[test]
fn panes_reports_a_server_that_is_gone() {
    let fx = FakeSlurm::with_jobs(&[Job::new(147845, "sinteractive:web")]);
    cmd(&fx, &["panes", "web"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "could not list the panes of session 147845 on node01: There is no active session!",
        ));
}

#[test]
fn peek_defaults_to_terminal_0_without_a_listing() {
    let fx = fixture();
    cmd(&fx, &["peek", "web"])
        .assert()
        .success()
        .stdout("shell line\n");
    assert_eq!(
        zellij_calls(&fx),
        ["action dump-screen -p terminal_0 --full"]
    );
}

#[test]
fn peek_takes_a_pane_id_name_or_focused() {
    let fx = fixture();
    cmd(&fx, &["peek", "web", "--pane", "terminal_3"])
        .assert()
        .success()
        .stdout("make: *** [all] Error 2\n");
    assert_eq!(
        zellij_calls(&fx),
        ["action dump-screen -p terminal_3 --full"]
    );

    // A bare number is an id too.
    cmd(&fx, &["peek", "web", "--pane", "3"])
        .assert()
        .success()
        .stdout("make: *** [all] Error 2\n");

    // A name needs the listing first.
    let fx = fixture();
    cmd(&fx, &["peek", "web", "--pane", "build"])
        .assert()
        .success()
        .stdout("make: *** [all] Error 2\n");
    assert_eq!(
        zellij_calls(&fx),
        [
            "action list-panes --json --all",
            "action dump-screen -p terminal_3 --full"
        ]
    );

    let fx = fixture();
    cmd(&fx, &["peek", "web", "--pane", "focused"])
        .assert()
        .success()
        .stdout("shell line\n");
    assert_eq!(
        zellij_calls(&fx)[1],
        "action dump-screen -p terminal_0 --full"
    );

    cmd(&fx, &["peek", "web", "--pane", "nope"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no pane named 'nope'"));
}

#[test]
fn send_defaults_to_terminal_0_and_takes_a_pane() {
    let fx = fixture();
    cmd(&fx, &["send", "web", "ls"]).assert().success();
    assert_eq!(
        zellij_calls(&fx),
        [
            "action write-chars -p terminal_0 ls",
            "action write -p terminal_0 13"
        ]
    );

    let fx = fixture();
    cmd(&fx, &["send", "web", "ls", "--pane", "build"])
        .assert()
        .success();
    assert_eq!(
        zellij_calls(&fx),
        [
            "action list-panes --json --all",
            "action write-chars -p terminal_3 ls",
            "action write -p terminal_3 13"
        ]
    );
}

#[test]
fn exec_runs_in_a_new_pane_and_returns_output_and_exit_code() {
    let fx = fixture();
    let work = fx.tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let out = cmd(
        &fx,
        &[
            "exec",
            "web",
            "pwd; echo out; echo err >&2; exit 3",
            "--cwd",
            work.to_str().unwrap(),
            "--json",
        ],
    )
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(3), "the exit code is the command's");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["job_id"], 147845);
    assert_eq!(v["node"], "node01");
    assert_eq!(v["pane_id"], "terminal_5");
    assert_eq!(v["exit_code"], 3);
    assert_eq!(v["timed_out"], false);
    assert_eq!(v["truncated"], false);
    let lines: Vec<&str> = v["output"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].ends_with("/work"), "{lines:?}");
    assert!(lines.contains(&"out") && lines.contains(&"err"));

    let calls = zellij_calls(&fx);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls[0].starts_with("run --name agent-")
            && calls[0].contains(" --no-focus --floating --cwd ")
            && calls[0].contains(" -- bash -c "),
        "{}",
        calls[0]
    );
    assert_eq!(calls[1], "action close-pane -p terminal_5");
}

#[test]
fn exec_plain_prints_the_output_and_exits_with_the_code() {
    let fx = fixture();
    cmd(&fx, &["exec", "web", "echo hello; exit 4"])
        .assert()
        .code(4)
        .stdout("hello\n");
    cmd(&fx, &["exec", "web", "echo fine"])
        .assert()
        .success()
        .stdout("fine\n");
    cmd(&fx, &["exec", "web", "  "])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("COMMAND is empty"));
}

#[test]
fn exec_times_out_kills_the_pane_and_keeps_what_it_printed() {
    let fx = fixture();
    let out = cmd(
        &fx,
        &[
            "exec",
            "web",
            "echo started; sleep 4",
            "--timeout",
            "1",
            "--json",
        ],
    )
    .output()
    .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["timed_out"], true);
    assert_eq!(v["exit_code"], Value::Null);
    assert_eq!(v["output"][0], "started");
    assert_eq!(
        zellij_calls(&fx).last().unwrap(),
        "action close-pane -p terminal_5"
    );
}

#[test]
fn exec_keep_leaves_the_pane_open() {
    let fx = fixture();
    cmd(&fx, &["exec", "web", "true", "--keep"])
        .assert()
        .success();
    assert!(
        zellij_calls(&fx).iter().all(|c| !c.contains("close-pane")),
        "{:?}",
        zellij_calls(&fx)
    );
}

#[test]
fn exec_reports_a_zellij_that_refuses() {
    let fx = fixture();
    fx.write("zellij.fail", "There is no active session!\n");
    cmd(&fx, &["exec", "web", "true"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "could not run in session 147845 on node01: There is no active session!",
        ));
}
