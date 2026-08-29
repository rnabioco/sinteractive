//! `sinteractive install-claude` against a throwaway `CLAUDE_CONFIG_DIR`,
//! with `SINTERACTIVE_SHARE` pointing at this checkout. `PATH` carries no
//! `claude`, so the MCP server lands in `<CLAUDE_CONFIG_DIR>/.claude.json`.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{repo_root, FakeSlurm};
use predicates::prelude::*;
use serde_json::Value;

/// What the install writes as the command: the absolute path of the binary
/// under test, so Claude Code runs this one whatever its PATH holds.
fn exe() -> String {
    assert_cmd::cargo::cargo_bin("sinteractive")
        .canonicalize()
        .expect("sinteractive binary")
        .to_str()
        .unwrap()
        .to_owned()
}

fn install(fx: &FakeSlurm) -> assert_cmd::assert::Assert {
    fx.sinteractive()
        .env("SINTERACTIVE_SHARE", repo_root())
        .args(["claude", "install"])
        .assert()
}

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(p).expect("read json")).expect("valid json")
}

fn backups(dir: &Path, stem: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&format!("{stem}.bak-")))
        .collect();
    v.sort();
    v
}

#[test]
fn fresh_install_creates_skills_hooks_settings_and_mcp() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    install(&fx).success().stdout(
        predicate::str::contains("Installed the Claude Code skills")
            .and(predicate::str::contains("hpc-compute"))
            .and(predicate::str::contains("git-workflow"))
            .and(predicate::str::contains(
                "Registered the hooks and statusline",
            ))
            .and(predicate::str::contains("Registered the MCP server in")),
    );

    // Skills: every directory with a SKILL.md, all of its .md files.
    for skill in [
        "hpc-compute",
        "hpc-software",
        "hpc-storage",
        "slurm-batch",
        "slurm-discovery",
        "git-workflow",
    ] {
        assert!(
            claude.join("skills").join(skill).join("SKILL.md").is_file(),
            "{skill}"
        );
    }
    assert!(claude.join("skills/hpc-compute/alpine.md").is_file());
    assert!(claude.join("skills/hpc-compute/bodhi.md").is_file());

    // No hook scripts: the hooks are subcommands of the binary.
    assert!(fs::read_dir(claude.join("hooks"))
        .map(|d| d.count() == 0)
        .unwrap_or(true));

    // settings.json: both hooks and the statusline, no backup (new file).
    let settings = read_json(&claude.join("settings.json"));
    assert_eq!(
        settings["hooks"]["SessionStart"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        settings["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        settings["statusLine"]["command"],
        format!("{} claude statusline", exe())
    );
    assert_eq!(settings["statusLine"]["refreshInterval"], 5);
    assert!(backups(&claude, "settings.json").is_empty());

    // MCP server in the config dir's .claude.json.
    let cfg = read_json(&claude.join(".claude.json"));
    assert_eq!(cfg["mcpServers"]["sinteractive"]["command"], exe());
    assert_eq!(
        cfg["mcpServers"]["sinteractive"]["args"],
        serde_json::json!(["claude", "mcp"])
    );
    assert_eq!(cfg["mcpServers"]["sinteractive"]["type"], "stdio");
    // No shim was touched.
    assert!(fx.calls().is_empty());
}

#[test]
fn second_run_is_a_no_op() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    install(&fx).success();
    let settings_before = fs::read_to_string(claude.join("settings.json")).unwrap();
    let cfg_before = fs::read_to_string(claude.join(".claude.json")).unwrap();

    install(&fx).success().stdout(
        predicate::str::contains("already registered; your settings were left alone").and(
            predicate::str::contains("The MCP server is already registered"),
        ),
    );
    assert_eq!(
        fs::read_to_string(claude.join("settings.json")).unwrap(),
        settings_before
    );
    assert_eq!(
        fs::read_to_string(claude.join(".claude.json")).unwrap(),
        cfg_before
    );
    assert!(backups(&claude, "settings.json").is_empty());
    assert!(backups(&claude, ".claude.json").is_empty());
}

#[test]
fn existing_settings_are_preserved_and_backed_up() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    let original = r#"{
  "permissions": { "allow": ["Bash(ls:*)"] },
  "statusLine": { "type": "command", "command": "my-statusline" },
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "echo mine" } ] }
    ]
  },
  "model": "opus"
}
"#;
    fs::write(claude.join("settings.json"), original).unwrap();
    fs::set_permissions(
        claude.join("settings.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();

    install(&fx).success().stdout(
        predicate::str::contains("Registered the hooks in")
            .and(predicate::str::contains("the previous version is at")),
    );

    let settings = read_json(&claude.join("settings.json"));
    // Key order and user content survive.
    let keys: Vec<&String> = settings.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["permissions", "statusLine", "hooks", "model"]);
    assert_eq!(settings["permissions"]["allow"][0], "Bash(ls:*)");
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["statusLine"]["command"], "my-statusline");
    let start = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(start.len(), 2);
    assert_eq!(start[0]["hooks"][0]["command"], "echo mine");
    assert_eq!(
        start[1]["hooks"][0]["command"],
        format!("{} claude hook session-start", exe())
    );
    assert_eq!(
        settings["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Mode kept, backup holds the original text.
    assert_eq!(
        fs::metadata(claude.join("settings.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let b = backups(&claude, "settings.json");
    assert_eq!(b.len(), 1);
    assert_eq!(fs::read_to_string(claude.join(&b[0])).unwrap(), original);

    // A hook already registered by script name (different path, no bash
    // prefix) in settings.local.json is not added again.
    let fx2 = FakeSlurm::new();
    let claude2 = fx2.claude_dir();
    fs::write(
        claude2.join("settings.local.json"),
        r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"/opt/hooks/sinteractive-walltime-guard.sh"}]}]}}"#,
    )
    .unwrap();
    install(&fx2).success();
    let settings = read_json(&claude2.join("settings.json"));
    assert!(settings["hooks"].get("UserPromptSubmit").is_none());
    assert_eq!(
        settings["hooks"]["SessionStart"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn invalid_settings_json_is_refused_and_untouched() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    let broken = "{ \"hooks\": [ oops\n";
    fs::write(claude.join("settings.json"), broken).unwrap();

    install(&fx)
        .code(2)
        .stderr(predicate::str::contains("is not valid JSON"))
        .stdout(predicate::str::contains("Merge this into"));
    assert_eq!(
        fs::read_to_string(claude.join("settings.json")).unwrap(),
        broken
    );
    assert!(backups(&claude, "settings.json").is_empty());
    // Skills and the MCP server were still installed.
    assert!(claude.join("skills/hpc-compute/SKILL.md").is_file());
    assert!(claude.join(".claude.json").is_file());

    // Same for a broken .claude.json.
    let fx2 = FakeSlurm::new();
    let claude2 = fx2.claude_dir();
    fs::write(claude2.join(".claude.json"), broken).unwrap();
    install(&fx2)
        .code(2)
        .stderr(predicate::str::contains(".claude.json is not valid JSON"));
    assert_eq!(
        fs::read_to_string(claude2.join(".claude.json")).unwrap(),
        broken
    );
    assert!(claude2.join("settings.json").is_file());
}

#[test]
fn symlinked_settings_json_edits_the_target() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    let dotfiles = fx.home_dir().join("dotfiles");
    fs::create_dir_all(&dotfiles).unwrap();
    let target = dotfiles.join("claude-settings.json");
    fs::write(&target, "{\"model\": \"opus\"}\n").unwrap();
    std::os::unix::fs::symlink(&target, claude.join("settings.json")).unwrap();

    install(&fx)
        .success()
        .stdout(predicate::str::contains(format!(
            "Registered the hooks and statusline in {}",
            target.canonicalize().unwrap().display()
        )));

    assert!(claude
        .join("settings.json")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    let settings = read_json(&target);
    assert_eq!(settings["model"], "opus");
    assert!(settings["hooks"]["SessionStart"].is_array());
    assert!(settings["statusLine"].is_object());
    assert_eq!(backups(&dotfiles, "claude-settings.json").len(), 1);
    assert!(backups(&claude, "settings.json").is_empty());
}

#[test]
fn stale_bodhi_skills_are_removed_after_their_successor_lands() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    fs::create_dir_all(claude.join("skills/bodhi-compute")).unwrap();
    fs::write(claude.join("skills/bodhi-compute/SKILL.md"), "old").unwrap();
    fs::create_dir_all(claude.join("skills/bodhi-other")).unwrap();
    install(&fx).success().stdout(predicate::str::contains(
        "Removed the stale bodhi-compute skill",
    ));
    assert!(!claude.join("skills/bodhi-compute").exists());
    assert!(claude.join("skills/bodhi-other").is_dir());
    assert!(claude.join("skills/hpc-compute/SKILL.md").is_file());
}

#[test]
fn compat_flag_still_works() {
    let fx = FakeSlurm::new();
    fx.sinteractive()
        .env("SINTERACTIVE_SHARE", repo_root())
        .arg("--install-claude")
        .assert()
        .success()
        .stderr(predicate::str::contains("deprecated"));
    assert!(fx.claude_dir().join("settings.json").is_file());
}

/// A settings.json written by an earlier install: the entries are renamed in
/// place rather than duplicated, and the user's own hook is untouched.
#[test]
fn an_earlier_installs_entries_are_renamed_not_duplicated() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    fs::create_dir_all(&claude).unwrap();
    fs::write(
        claude.join("settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo mine"}]},{"hooks":[{"type":"command","command":"sinteractive hook session-start","timeout":10}]}],"UserPromptSubmit":[{"hooks":[{"type":"command","command":"sinteractive hook prompt","timeout":10}]}]},"statusLine":{"type":"command","command":"sinteractive statusline","refreshInterval":5}}"#,
    )
    .unwrap();
    install(&fx).success();

    let settings = read_json(&claude.join("settings.json"));
    let start = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(start.len(), 2, "renamed in place, not appended to");
    assert_eq!(start[0]["hooks"][0]["command"], "echo mine");
    assert_eq!(
        start[1]["hooks"][0]["command"],
        format!("{} claude hook session-start", exe())
    );
    let prompt = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(prompt.len(), 1);
    assert_eq!(
        prompt[0]["hooks"][0]["command"],
        format!("{} claude hook prompt", exe())
    );
    assert_eq!(
        settings["statusLine"]["command"],
        format!("{} claude statusline", exe())
    );

    // Nothing left to do the second time.
    install(&fx).success();
    assert_eq!(read_json(&claude.join("settings.json")), settings);
}

#[test]
fn legacy_script_hooks_are_replaced_by_native_ones() {
    let fx = FakeSlurm::new();
    let claude = fx.claude_dir();
    fs::create_dir_all(claude.join("hooks")).unwrap();
    fs::write(
        claude.join("hooks/sinteractive-walltime-guard.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    fs::write(
        claude.join("settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"bash ~/.claude/hooks/sinteractive-session-context.sh","timeout":10}]}],"UserPromptSubmit":[{"hooks":[{"type":"command","command":"echo keep"}]},{"hooks":[{"type":"command","command":"bash ~/.claude/hooks/sinteractive-walltime-guard.sh","timeout":10}]}]}}"#,
    )
    .unwrap();
    install(&fx)
        .success()
        .stdout(predicate::str::contains("Removed the 0.x hook script"));
    assert!(!claude.join("hooks/sinteractive-walltime-guard.sh").exists());
    let settings = read_json(&claude.join("settings.json"));
    let start = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(start.len(), 1);
    assert_eq!(
        start[0]["hooks"][0]["command"],
        format!("{} claude hook session-start", exe())
    );
    let prompt = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(prompt.len(), 2);
    assert_eq!(prompt[0]["hooks"][0]["command"], "echo keep");
    assert_eq!(
        prompt[1]["hooks"][0]["command"],
        format!("{} claude hook prompt", exe())
    );
    // Idempotent afterwards.
    install(&fx).success();
    let again = read_json(&claude.join("settings.json"));
    assert_eq!(again, settings);
}
