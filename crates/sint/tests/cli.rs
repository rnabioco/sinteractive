//! End-to-end checks of the clap surface through the real binary.

mod common;

use common::FakeSlurm;
use predicates::prelude::*;

#[test]
fn help_lists_the_subcommands() {
    let fx = FakeSlurm::new();
    fx.sinteractive().arg("--help").assert().success().stdout(
        predicate::str::contains("Usage:")
            .and(predicate::str::contains("attach"))
            .and(predicate::str::contains("session"))
            .and(predicate::str::contains("status"))
            .and(predicate::str::contains("list"))
            .and(predicate::str::contains("cancel"))
            .and(predicate::str::contains("claude"))
            .and(predicate::str::contains("gen"))
            // Hidden verbs stay hidden.
            .and(predicate::str::contains("__job").not()),
    );
    fx.sinteractive().arg("-h").assert().success();
    fx.sinteractive()
        .args(["help", "status"])
        .assert()
        .success();
}

/// The point of the grouping: `--help` names the verbs a person types, and
/// the Claude Code wiring and the generators appear once each, as groups.
#[test]
fn help_hides_the_grouped_verbs() {
    let fx = FakeSlurm::new();
    // `statusline`, `completions` and `schema` are deliberately not checked
    // here: the two group descriptions name their members, so the words are
    // in the help even though the commands are not.
    fx.sinteractive().arg("--help").assert().success().stdout(
        predicate::str::contains("install-claude")
            .not()
            .and(predicate::str::contains("agent-context").not())
            .and(predicate::str::contains("snapshot").not())
            .and(predicate::str::contains("refresh").not()),
    );
    // The groups list their own members.
    fx.sinteractive()
        .args(["claude", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("install")
                .and(predicate::str::contains("context"))
                .and(predicate::str::contains("hook"))
                .and(predicate::str::contains("statusline"))
                .and(predicate::str::contains("mcp")),
        );
    fx.sinteractive()
        .args(["session", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("ensure")
                .and(predicate::str::contains("peek"))
                .and(predicate::str::contains("send"))
                .and(predicate::str::contains("events")),
        );
    fx.sinteractive()
        .args(["gen", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("completions")
                .and(predicate::str::contains("man"))
                .and(predicate::str::contains("schema")),
        );
}

/// Nothing that already invokes sinteractive has to be edited in step.
#[test]
fn the_pre_grouping_names_still_resolve() {
    let fx = FakeSlurm::new();
    for args in [
        vec!["completions", "bash"],
        vec!["gen", "completions", "bash"],
        vec!["man"],
        vec!["schema"],
        vec!["peek", "--help"],
        vec!["send", "--help"],
        vec!["events", "--help"],
        vec!["ensure", "--help"],
    ] {
        fx.sinteractive().args(&args).assert().success();
    }
}

#[test]
fn version_prints_the_workspace_version() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .arg("--version")
        .assert()
        .success()
        .stdout("sinteractive 1.2.0\n");
    fx.sinteractive()
        .arg("-V")
        .assert()
        .success()
        .stdout("sinteractive 1.2.0\n");
}

#[test]
fn compat_flags_warn_on_stderr() {
    let fx = FakeSlurm::new();
    // `list` itself is still unimplemented in this phase, so only the
    // deprecation warning is asserted, not the exit status.
    fx.sinteractive()
        .args(["--list", "--json"])
        .assert()
        .stderr(predicate::str::contains("deprecated"));
    fx.sinteractive()
        .args(["list", "--json"])
        .assert()
        .stderr(predicate::str::contains("deprecated").not());
}

#[test]
fn bundled_short_flags_are_a_usage_error() {
    let fx = FakeSlurm::new();
    fx.sinteractive().arg("-la").assert().code(2).stderr(
        predicate::str::contains("unrecognized option '-la'")
            .and(predicate::str::contains("-l (--list)")),
    );
    fx.sinteractive()
        .arg("-n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("-n requires a NAME argument"));
}

#[test]
fn unknown_launch_flags_are_not_a_clap_error() {
    let fx = FakeSlurm::new();
    // They belong to sbatch; the launch then fails only because launch is
    // not implemented yet in this phase, never with a clap usage error.
    fx.sinteractive()
        .args(["--gres=gpu:1", "-n", "web", "--exclusive"])
        .assert()
        .stderr(predicate::str::contains("unexpected argument").not());
}

#[test]
fn completions_emit_a_script() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["gen", "completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sinteractive"));
    fx.sinteractive()
        .args(["gen", "completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sinteractive"));
}

#[test]
fn man_emits_roff() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .args(["gen", "man"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH sinteractive"));
}
