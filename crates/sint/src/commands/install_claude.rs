//! `sinteractive claude install` — install the Claude Code skills, hooks,
//! statusline and MCP server for the current user.
//!
//! Ports `find_claude_assets`, `install_claude` and `register_claude_hooks`
//! from the 0.x script (lines 1601-1822) without jq: the settings merge is
//! done with `serde_json` (key order preserved) under the same rules —
//! additive, idempotent, compared as JSON, written only when something
//! changed, backed up first, refused when the file does not parse.
//!
//! Exit codes: 0 done (or nothing to do), 1 no assets, 2 a settings file was
//! refused (left alone; everything else was still installed).

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use sint_core::color::Palette;
use sint_core::config::ColorMode;

/// Skill directories shipped before the 2026-08 rename, removed when their
/// `hpc-*` successor has just been installed.
const STALE_SKILLS: &[(&str, &str)] = &[
    ("bodhi-compute", "hpc-compute"),
    ("bodhi-software", "hpc-software"),
    ("bodhi-storage", "hpc-storage"),
];

pub fn run() -> Result<i32> {
    let p = Palette::for_fd(ColorMode::from_env(), 1);
    let e = Palette::for_fd(ColorMode::from_env(), 2);

    let Some(assets) = find_assets() else {
        eprintln!(
            "{}{}sinteractive:{}{} could not find the Claude Code assets.{}",
            e.err, e.bold, e.reset, e.err, e.reset
        );
        eprintln!();
        eprintln!(
            "{}Looked for claude/settings-snippet.json and skills/ beside this binary, in its{}",
            e.dim, e.reset
        );
        eprintln!(
            "{}../share/sinteractive, and in the checkout it was built from.{}",
            e.dim, e.reset
        );
        eprintln!(
            "{}If sinteractive was installed without them, reinstall from a{}",
            e.dim, e.reset
        );
        eprintln!(
            "{}checkout (make install), or point SINTERACTIVE_SHARE at one:{}",
            e.dim, e.reset
        );
        eprintln!();
        eprintln!(
            "  {}SINTERACTIVE_SHARE=/path/to/sinteractive sinteractive claude install{}",
            e.key, e.reset
        );
        return Ok(1);
    };

    let claude_dir = claude_config_dir();
    let exe = exe_command();
    fs::create_dir_all(claude_dir.join("hooks"))
        .with_context(|| format!("could not create {}", claude_dir.join("hooks").display()))?;

    let names = install_skills(&assets, &claude_dir)?;
    for (old, new) in remove_stale_skills(&claude_dir, &names)? {
        println!(
            "{}Removed the stale {old} skill (renamed to {new}){}",
            p.dim, p.reset
        );
    }
    for old in remove_legacy_hook_scripts(&claude_dir)? {
        println!(
            "{}Removed the 0.x hook script {} (hooks are `sinteractive hook …` now){}",
            p.dim,
            old.display(),
            p.reset
        );
    }

    println!(
        "{}✓{} Installed the Claude Code skills ({}{}{}) into {}{}{}",
        p.ok,
        p.reset,
        p.key,
        names.join(", "),
        p.reset,
        p.id,
        claude_dir.display(),
        p.reset
    );
    println!("  {}from {}{}", p.dim, assets.display(), p.reset);
    println!();

    let mut refused = false;

    // settings.json: hooks + statusLine.
    let snippet_path = assets.join("claude/settings-snippet.json");
    match register_settings(&claude_dir, &snippet_path, &exe) {
        Ok(SettingsOutcome::Unchanged) => println!(
            "{}✓{} {}The hooks and statusline are already registered; your settings were left alone.{}",
            p.ok, p.reset, p.dim, p.reset
        ),
        Ok(SettingsOutcome::Written {
            path,
            backup,
            hooks,
            statusline,
            migrated,
        }) => {
            let what = match (hooks, statusline, migrated) {
                (true, true, _) => "Registered the hooks and statusline",
                (true, false, _) => "Registered the hooks",
                (false, true, _) => "Registered the statusline",
                _ => "Pointed the hooks and statusline at this binary",
            };
            println!("{}✓{} {what} in {}{}{}", p.ok, p.reset, p.id, path.display(), p.reset);
            if let Some(b) = backup {
                println!("  {}the previous version is at {}{}", p.dim, b.display(), p.reset);
            }
        }
        Err(Refused(msg)) => {
            eprintln!("{}{}sinteractive:{}{} {msg}{}", e.err, e.bold, e.reset, e.err, e.reset);
            println!(
                "{}The hooks only take effect once they are registered.{} Merge this into",
                p.warn, p.reset
            );
            println!(
                "{}{}{} (keeping any hooks already there):",
                p.id,
                claude_dir.join("settings.json").display(),
                p.reset
            );
            println!();
            if let Ok(s) = fs::read_to_string(&snippet_path) {
                for line in s.lines() {
                    println!("  {line}");
                }
            }
            println!();
            refused = true;
        }
    }

    // MCP server.
    match register_mcp(&claude_dir, &exe) {
        Ok(McpOutcome::AddedViaCli) => println!(
            "{}✓{} Registered the MCP server ({}claude mcp add --scope user sinteractive -- {exe} claude mcp{})",
            p.ok, p.reset, p.key, p.reset
        ),
        Ok(McpOutcome::AlreadyRegistered) => println!(
            "{}✓{} {}The MCP server is already registered.{}",
            p.ok, p.reset, p.dim, p.reset
        ),
        Ok(McpOutcome::Written {
            path,
            backup,
            updated,
        }) => {
            let what = if updated {
                "Pointed the MCP server at this binary"
            } else {
                "Registered the MCP server"
            };
            println!(
                "{}✓{} {what} in {}{}{}",
                p.ok,
                p.reset,
                p.id,
                path.display(),
                p.reset
            );
            if let Some(b) = backup {
                println!(
                    "  {}the previous version is at {}{}",
                    p.dim,
                    b.display(),
                    p.reset
                );
            }
        }
        Ok(McpOutcome::CliFailed(msg)) => {
            println!(
                "{}claude mcp add did not register the server:{} {msg}",
                p.warn, p.reset
            );
            println!(
                "  {}run it by hand: claude mcp add --scope user sinteractive -- {exe} claude mcp{}",
                p.dim, p.reset
            );
        }
        Err(Refused(msg)) => {
            eprintln!(
                "{}{}sinteractive:{}{} {msg}{}",
                e.err, e.bold, e.reset, e.err, e.reset
            );
            refused = true;
        }
    }

    println!();
    Ok(if refused { 2 } else { 0 })
}

// ---- assets ----------------------------------------------------------------

/// `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
fn claude_config_dir() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => home_dir().join(".claude"),
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Locate the directory holding `claude/settings-snippet.json` and `skills/`: an explicit
/// `SINTERACTIVE_SHARE`, the share dir beside the installed binary
/// (`bin/../share/sinteractive`), the binary's own directory, then the
/// checkout the binary was built in (`target/debug/` and
/// `target/<triple>/release/` are two or three levels below the root).
fn find_assets() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(s) = std::env::var_os("SINTERACTIVE_SHARE") {
        if !s.is_empty() {
            candidates.push(PathBuf::from(s));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe = fs::canonicalize(&exe).unwrap_or(exe);
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("../share/sinteractive"));
            candidates.push(dir.to_path_buf());
            let mut up = dir.parent();
            for _ in 0..3 {
                if let Some(d) = up {
                    candidates.push(d.to_path_buf());
                    up = d.parent();
                }
            }
        }
    }
    candidates.into_iter().find(|r| has_assets(r))
}

/// `hpc-compute` is the probe rather than `skills/` itself, so an empty
/// `skills/` left by a half-finished install does not pass; `bodhi-compute`
/// accepts a tree from before the 2026-08 rename.
fn has_assets(root: &Path) -> bool {
    root.join("claude/settings-snippet.json").is_file()
        && (root.join("skills/hpc-compute").is_dir() || root.join("skills/bodhi-compute").is_dir())
}

/// Copy every `skills/<name>/*.md` into `<claude_dir>/skills/<name>/`.
/// Returns the installed names, sorted.
fn install_skills(assets: &Path, claude_dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let skills = assets.join("skills");
    let mut dirs: Vec<PathBuf> = match fs::read_dir(&skills) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join("SKILL.md").is_file())
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort();
    for src in dirs {
        let name = src
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string();
        let dst = claude_dir.join("skills").join(&name);
        fs::create_dir_all(&dst).with_context(|| format!("could not create {}", dst.display()))?;
        for f in md_files(&src)? {
            let target = dst.join(f.file_name().unwrap_or_default());
            fs::copy(&f, &target).with_context(|| {
                format!("could not copy {} to {}", f.display(), target.display())
            })?;
        }
        names.push(name);
    }
    Ok(names)
}

fn md_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("could not read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension() == Some(OsStr::new("md")))
        .collect();
    v.sort();
    Ok(v)
}

/// Remove `bodhi-*` skills whose `hpc-*` successor was just installed.
fn remove_stale_skills(claude_dir: &Path, installed: &[String]) -> Result<Vec<(String, String)>> {
    let mut removed = Vec::new();
    for (old, new) in STALE_SKILLS {
        let old_dir = claude_dir.join("skills").join(old);
        let new_dir = claude_dir.join("skills").join(new);
        if old_dir.is_dir() && new_dir.is_dir() && installed.iter().any(|n| n == new) {
            fs::remove_dir_all(&old_dir)
                .with_context(|| format!("could not remove {}", old_dir.display()))?;
            removed.push((old.to_string(), new.to_string()));
        }
    }
    Ok(removed)
}

/// Remove the 0.x hook scripts from `<claude_dir>/hooks/`; the hooks are
/// subcommands of the binary now (`sinteractive hook …`). Returns what was
/// removed.
fn remove_legacy_hook_scripts(claude_dir: &Path) -> Result<Vec<PathBuf>> {
    let dir = claude_dir.join("hooks");
    let mut removed = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(removed);
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with("sinteractive-") && name.ends_with(".sh") && path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("could not remove {}", path.display()))?;
            removed.push(path);
        }
    }
    removed.sort();
    Ok(removed)
}

// ---- settings.json ---------------------------------------------------------

/// A settings file that was left alone, with the reason.
#[derive(Debug)]
pub struct Refused(pub String);

#[derive(Debug)]
pub enum SettingsOutcome {
    Unchanged,
    Written {
        path: PathBuf,
        backup: Option<PathBuf>,
        hooks: bool,
        statusline: bool,
        /// Entries an earlier install wrote were renamed onto the current
        /// `<exe> claude …` spelling: the grouped verb, this binary's path.
        migrated: bool,
    },
}

/// How the hooks, statusline and MCP server invoke this binary: its absolute
/// path, so Claude Code runs the copy that installed them whatever its PATH
/// holds. Hooks and the MCP server start from a non-interactive shell that
/// knows no aliases, and an older `sinteractive` earlier on PATH — the 0.x
/// script in /usr/local/bin, say — would take the call and fail it. The bare
/// name is the fallback when the running executable cannot be found out.
fn exe_command() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_else(|| "sinteractive".into())
}

/// The statusLine entry added when the user has none.
fn statusline_value(exe: &str) -> Value {
    json!({
        "type": "command",
        "command": format!("{exe} claude statusline"),
        "refreshInterval": 5
    })
}

/// Merge the snippet's hooks and the statusline into
/// `<claude_dir>/settings.json` (following a symlink to its target), with
/// every command spelled as `<exe> claude …`.
pub fn register_settings(
    claude_dir: &Path,
    snippet_path: &Path,
    exe: &str,
) -> Result<SettingsOutcome, Refused> {
    let mut snippet = load_object(snippet_path)
        .map_err(|m| Refused(format!("could not read {}: {m}", snippet_path.display())))?;
    // The snippet names the bare `sinteractive`; what lands in the settings
    // is this binary.
    migrate_commands(&mut snippet, exe);

    let link = claude_dir.join("settings.json");
    let settings = resolve_settings_path(&link)?;
    let base = load_object(&settings).map_err(|m| {
        Refused(format!(
            "{} is not valid JSON ({m}), so it was left alone.",
            settings.display()
        ))
    })?;
    // The .local variant is read only to see what is already registered
    // there, so an unreadable one costs nothing but that knowledge.
    let other = load_object(&claude_dir.join("settings.local.json")).unwrap_or_default();

    let mut merged = base.clone();
    // Bring an earlier install's entries up to date first — the ungrouped
    // `sinteractive hook …` / `sinteractive statusline` spellings, and any
    // that name the binary by a different path (or by no path) — so the
    // merge below sees the same spellings the snippet carries.
    let migrated = migrate_commands(&mut merged, exe);
    let hooks = merge_hooks(&mut merged, &snippet, &other).map_err(|m| {
        Refused(format!(
            "could not merge into {}: {m}; it was left alone.",
            settings.display()
        ))
    })?;
    let statusline = merge_statusline(&mut merged, exe);

    if !hooks && !statusline && !migrated {
        return Ok(SettingsOutcome::Unchanged);
    }
    let backup = write_json(&settings, &Value::Object(merged))
        .map_err(|m| Refused(format!("could not write {}: {m}", settings.display())))?;
    Ok(SettingsOutcome::Written {
        path: settings,
        backup,
        hooks,
        statusline,
        migrated,
    })
}

/// Follow a symlinked settings.json (dotfile repos do this) so the merge
/// lands on the real file instead of replacing the link with a copy.
fn resolve_settings_path(path: &Path) -> Result<PathBuf, Refused> {
    if path.symlink_metadata().is_err() {
        return Ok(path.to_path_buf());
    }
    fs::canonicalize(path).map_err(|e| {
        Refused(format!(
            "{} could not be resolved ({e}), so it was left alone.",
            path.display()
        ))
    })
}

/// Read a JSON object from `path`. Missing or empty → `{}`. Anything that
/// does not parse, or is not an object, is an error.
fn load_object(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(e.to_string()),
    };
    if text.trim().is_empty() {
        if text.is_empty() {
            return Ok(Map::new());
        }
        return Err("empty document".into());
    }
    match serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())? {
        Value::Object(m) => Ok(m),
        _ => Err("top level is not an object".into()),
    }
}

/// Every sinteractive hook identity named in a `command` string anywhere
/// under `v` (jq: `.. | objects | .command? | strings`).
fn hook_scripts(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(cmd)) = m.get("command") {
                if let Some(name) = hook_identity(cmd) {
                    out.insert(name.to_string());
                }
            }
            for child in m.values() {
                hook_scripts(child, out);
            }
        }
        Value::Array(a) => {
            for child in a {
                hook_scripts(child, out);
            }
        }
        _ => {}
    }
}

/// Which sinteractive hook a `command` string is, if any: the current form
/// (`sinteractive claude hook session-start` / `… hook prompt`), the
/// ungrouped spelling an earlier install wrote (`sinteractive hook …`) and
/// the 0.x scripts (`…/sinteractive-session-context.sh`,
/// `…/sinteractive-walltime-guard.sh`) all map to the same identity, so an
/// upgrade replaces the old entry instead of adding a second hook.
fn hook_identity(command: &str) -> Option<&'static str> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"sinteractive(?:-session-context\.sh|-walltime-guard\.sh|\s+(?:claude\s+)?hook\s+(session-start|prompt))\b",
        )
        .unwrap()
    });
    let m = re.captures(command)?;
    Some(match m.get(1).map(|g| g.as_str()) {
        Some("session-start") => "session-start",
        Some("prompt") => "prompt",
        Some(_) => return None,
        None if m.get(0).unwrap().as_str().contains("session-context") => "session-start",
        None => "prompt",
    })
}

/// Spell `command` the way this install writes it — `<exe> claude <verb>` —
/// if it is one of ours: `sinteractive` by name or by any path, running the
/// current `claude hook …` / `claude statusline` or the ungrouped `hook …` /
/// `statusline` an earlier install wrote. Anything else — a hand-edited
/// command, a wrapper script, an argument we do not know — returns `None` and
/// is left alone; the hidden aliases keep the old verbs working either way.
/// `None` too when it is already spelled right.
fn renamed_command(command: &str, exe: &str) -> Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"^\s*(?:\S*/)?sinteractive\s+(?:claude\s+)?(?<verb>hook\s+(?:session-start|prompt)|statusline)\s*$",
        )
        .unwrap()
    });
    let c = re.captures(command)?;
    let verb = c["verb"].split_whitespace().collect::<Vec<_>>().join(" ");
    let new = format!("{exe} claude {verb}");
    (new != command).then_some(new)
}

/// Rewrite our `command` strings anywhere in the settings tree onto
/// `<exe> claude …`. Returns whether anything changed.
fn migrate_commands(settings: &mut Map<String, Value>, exe: &str) -> bool {
    // A plain loop rather than `any`: every entry has to be visited, so
    // short-circuiting on the first rename would leave the rest untouched.
    let mut changed = false;
    for v in settings.values_mut() {
        changed |= migrate_value(v, exe);
    }
    changed
}

fn migrate_value(value: &mut Value, exe: &str) -> bool {
    match value {
        Value::Object(m) => {
            let mut changed = false;
            if let Some(Value::String(cmd)) = m.get_mut("command") {
                if let Some(new) = renamed_command(cmd, exe) {
                    *cmd = new;
                    changed = true;
                }
            }
            for child in m.values_mut() {
                changed |= migrate_value(child, exe);
            }
            changed
        }
        Value::Array(a) => {
            let mut changed = false;
            for v in a {
                changed |= migrate_value(v, exe);
            }
            changed
        }
        _ => false,
    }
}

/// Whether `command` is a 0.x bash hook (`…/sinteractive-*.sh`).
fn is_legacy_hook(command: &str) -> bool {
    hook_identity(command).is_some() && command.contains(".sh")
}

/// Append the snippet's hook entries to `settings.hooks`, skipping any entry
/// whose script is already registered in `settings` or `other`. Returns
/// whether anything was added. Matching is by script name rather than by
/// the whole command, so a hand-edited path or a dropped `bash ` prefix
/// still counts as registered, and a half-registered pair gets only its
/// missing half.
pub fn merge_hooks(
    settings: &mut Map<String, Value>,
    snippet: &Map<String, Value>,
    other: &Map<String, Value>,
) -> Result<bool, String> {
    let Some(Value::Object(add)) = snippet.get("hooks") else {
        return Ok(false);
    };

    // Upgrade path: drop 0.x script entries so the native ones replace them.
    let mut changed = strip_legacy_hooks(settings);

    let mut registered = BTreeSet::new();
    for doc in [settings as &Map<String, Value>, other] {
        if let Some(h) = doc.get("hooks") {
            hook_scripts(h, &mut registered);
        }
    }

    for (event, entries) in add {
        let Value::Array(entries) = entries else {
            continue;
        };
        let new: Vec<Value> = entries
            .iter()
            .filter(|entry| {
                let mut scripts = BTreeSet::new();
                hook_scripts(entry, &mut scripts);
                scripts.is_disjoint(&registered)
            })
            .cloned()
            .collect();
        if new.is_empty() {
            continue;
        }
        let hooks = match settings.get("hooks") {
            None | Some(Value::Null) => {
                settings.insert("hooks".into(), Value::Object(Map::new()));
                settings.get_mut("hooks").unwrap()
            }
            Some(Value::Object(_)) => settings.get_mut("hooks").unwrap(),
            Some(_) => return Err("\"hooks\" is not an object".into()),
        };
        let hooks = hooks.as_object_mut().unwrap();
        match hooks.get_mut(event) {
            None | Some(Value::Null) => {
                hooks.insert(event.clone(), Value::Array(new));
            }
            Some(Value::Array(existing)) => existing.extend(new),
            Some(_) => return Err(format!("\"hooks.{event}\" is not an array")),
        }
        changed = true;
    }
    Ok(changed)
}

/// Remove hook entries whose every command is a 0.x `sinteractive-*.sh`
/// script. Entries mixing ours with the user's own commands are left alone.
/// Returns whether anything was removed.
fn strip_legacy_hooks(settings: &mut Map<String, Value>) -> bool {
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return false;
    };
    let mut changed = false;
    for entries in hooks.values_mut() {
        let Value::Array(entries) = entries else {
            continue;
        };
        let before = entries.len();
        entries.retain(|entry| {
            let cmds: Vec<&str> = entry
                .get("hooks")
                .and_then(Value::as_array)
                .map(|hs| {
                    hs.iter()
                        .filter_map(|h| h.get("command").and_then(Value::as_str))
                        .collect()
                })
                .unwrap_or_default();
            !(!cmds.is_empty() && cmds.iter().all(|c| is_legacy_hook(c)))
        });
        changed |= entries.len() != before;
    }
    changed
}

/// Add the statusline when the user has none. Returns whether it was added.
pub fn merge_statusline(settings: &mut Map<String, Value>, exe: &str) -> bool {
    if settings.contains_key("statusLine") {
        return false;
    }
    settings.insert("statusLine".into(), statusline_value(exe));
    true
}

/// Write `value` to `path` atomically: back the existing file up to
/// `<path>.bak-YYYYmmddHHMMSS`, keep its mode, replace by rename. Returns
/// the backup path when there was one.
fn write_json(path: &Path, value: &Value) -> Result<Option<PathBuf>, String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');

    let existing = fs::metadata(path).ok();
    let mode = existing
        .as_ref()
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o644);
    let backup = if existing.is_some() {
        let b = PathBuf::from(format!("{}.bak-{}", path.display(), timestamp()));
        fs::copy(path, &b).map_err(|e| format!("backup to {} failed: {e}", b.display()))?;
        Some(b)
    } else {
        None
    };

    let dir = path.parent().unwrap_or(Path::new("."));
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("settings"),
        std::process::id()
    ));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(backup)
}

fn timestamp() -> String {
    use time::macros::format_description;
    let fmt = format_description!("[year][month][day][hour][minute][second]");
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(&fmt).unwrap_or_else(|_| "0".into())
}

// ---- MCP server ------------------------------------------------------------

#[derive(Debug)]
pub enum McpOutcome {
    AddedViaCli,
    AlreadyRegistered,
    Written {
        path: PathBuf,
        backup: Option<PathBuf>,
        /// An entry an earlier install wrote was pointed at this binary,
        /// rather than a new one added.
        updated: bool,
    },
    CliFailed(String),
}

fn mcp_server_value(exe: &str) -> Value {
    json!({
        "type": "stdio",
        "command": exe,
        "args": ["claude", "mcp"]
    })
}

/// Register the MCP server: through `claude mcp add` when `claude` is on
/// PATH, else by editing `.claude.json` (in `CLAUDE_CONFIG_DIR` when set,
/// which is where Claude Code keeps it then, else `~`). An entry that is
/// already there goes through the file either way, since `claude mcp add`
/// refuses to touch one and it may name a binary that is not this one.
fn register_mcp(claude_dir: &Path, exe: &str) -> Result<McpOutcome, Refused> {
    if let Some(claude) = find_on_path("claude") {
        let out = Command::new(claude)
            .args([
                "mcp",
                "add",
                "--scope",
                "user",
                "sinteractive",
                "--",
                exe,
                "claude",
                "mcp",
            ])
            .output();
        match out {
            Ok(o) if o.status.success() => return Ok(McpOutcome::AddedViaCli),
            Ok(o) => {
                let msg = format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                if !msg.contains("already exists") {
                    return Ok(McpOutcome::CliFailed(msg.trim().to_string()));
                }
            }
            Err(e) => return Ok(McpOutcome::CliFailed(e.to_string())),
        }
    }

    let path = if std::env::var_os("CLAUDE_CONFIG_DIR").is_some_and(|d| !d.is_empty()) {
        claude_dir.join(".claude.json")
    } else {
        home_dir().join(".claude.json")
    };
    let path = resolve_settings_path(&path)?;
    let mut doc = load_object(&path).map_err(|m| {
        Refused(format!(
            "{} is not valid JSON ({m}), so it was left alone.",
            path.display()
        ))
    })?;
    let outcome = merge_mcp(&mut doc, exe).map_err(|m| {
        Refused(format!(
            "could not merge into {}: {m}; it was left alone.",
            path.display()
        ))
    })?;
    let updated = match outcome {
        McpMerge::Unchanged => return Ok(McpOutcome::AlreadyRegistered),
        McpMerge::Added => false,
        McpMerge::Repointed => true,
    };
    let backup = write_json(&path, &Value::Object(doc))
        .map_err(|m| Refused(format!("could not write {}: {m}", path.display())))?;
    Ok(McpOutcome::Written {
        path,
        backup,
        updated,
    })
}

#[derive(Debug, PartialEq)]
pub enum McpMerge {
    Unchanged,
    Added,
    Repointed,
}

/// Add `mcpServers.sinteractive` when absent, and point an entry an install
/// wrote — `sinteractive` by any path or none, running `claude mcp` or the
/// older `mcp` — at this binary, keeping whatever else it carries (`env`,
/// say). An entry configured any other way is the user's and stays.
pub fn merge_mcp(doc: &mut Map<String, Value>, exe: &str) -> Result<McpMerge, String> {
    let servers = match doc.get("mcpServers") {
        None | Some(Value::Null) => {
            doc.insert("mcpServers".into(), Value::Object(Map::new()));
            doc.get_mut("mcpServers").unwrap()
        }
        Some(Value::Object(_)) => doc.get_mut("mcpServers").unwrap(),
        Some(_) => return Err("\"mcpServers\" is not an object".into()),
    };
    let servers = servers.as_object_mut().unwrap();
    let wanted = mcp_server_value(exe);
    match servers.get_mut("sinteractive") {
        None => {
            servers.insert("sinteractive".into(), wanted);
            Ok(McpMerge::Added)
        }
        Some(entry) if is_our_mcp_entry(entry) => {
            let entry = entry.as_object_mut().unwrap();
            let mut changed = false;
            for key in ["command", "args"] {
                if entry.get(key) != wanted.get(key) {
                    entry.insert(key.into(), wanted[key].clone());
                    changed = true;
                }
            }
            Ok(if changed {
                McpMerge::Repointed
            } else {
                McpMerge::Unchanged
            })
        }
        Some(_) => Ok(McpMerge::Unchanged),
    }
}

/// Whether an `mcpServers` entry is one an install wrote: the command is
/// `sinteractive` (bare or by any path) and the arguments are `claude mcp`
/// or the pre-1.0 `mcp`.
fn is_our_mcp_entry(entry: &Value) -> bool {
    let ours = entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| Path::new(c).file_name() == Some(OsStr::new("sinteractive")));
    let args = entry.get("args").and_then(Value::as_array);
    ours && args.is_some_and(|a| *a == [json!("claude"), json!("mcp")] || *a == [json!("mcp")])
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| {
            p.is_file()
                && p.metadata()
                    .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(s: &str) -> Map<String, Value> {
        serde_json::from_str::<Value>(s)
            .unwrap()
            .as_object()
            .unwrap()
            .clone()
    }

    fn snippet() -> Map<String, Value> {
        obj(r#"{"hooks":{
            "SessionStart":[{"hooks":[{"type":"command","command":"sinteractive claude hook session-start","timeout":10}]}],
            "UserPromptSubmit":[{"hooks":[{"type":"command","command":"sinteractive claude hook prompt","timeout":10}]}]
        }}"#)
    }

    #[test]
    fn hook_identity_covers_legacy_and_native() {
        assert_eq!(
            hook_identity("bash ~/.claude/hooks/sinteractive-session-context.sh"),
            Some("session-start")
        );
        assert_eq!(
            hook_identity("/opt/x/sinteractive-walltime-guard.sh --x"),
            Some("prompt")
        );
        assert_eq!(
            hook_identity("sinteractive hook session-start"),
            Some("session-start")
        );
        assert_eq!(hook_identity("sinteractive  hook prompt"), Some("prompt"));
        assert_eq!(hook_identity("sinteractive statusline"), None);
        assert_eq!(hook_identity("my-hook.sh"), None);
        assert!(is_legacy_hook(
            "bash ~/.claude/hooks/sinteractive-walltime-guard.sh"
        ));
        assert!(!is_legacy_hook("sinteractive hook prompt"));
    }

    #[test]
    fn fresh_settings_get_both_hooks_in_order() {
        let mut s = Map::new();
        assert!(merge_hooks(&mut s, &snippet(), &Map::new()).unwrap());
        let keys: Vec<&String> = s["hooks"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["SessionStart", "UserPromptSubmit"]);
        assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        // Idempotent.
        assert!(!merge_hooks(&mut s, &snippet(), &Map::new()).unwrap());
    }

    #[test]
    fn existing_hooks_are_kept_and_half_registered_pairs_completed() {
        let mut s = obj(r#"{"permissions":{"allow":["Bash(ls:*)"]},"hooks":{
            "SessionStart":[{"hooks":[{"type":"command","command":"echo mine"}]},
                            {"hooks":[{"type":"command","command":"sinteractive-session-context.sh"}]}]
        }}"#);
        assert!(merge_hooks(&mut s, &snippet(), &Map::new()).unwrap());
        assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert_eq!(
            s["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "echo mine"
        );
        assert_eq!(s["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert_eq!(s["permissions"]["allow"][0], "Bash(ls:*)");
        let keys: Vec<&String> = s.keys().collect();
        assert_eq!(keys, ["permissions", "hooks"]);
    }

    #[test]
    fn scripts_registered_in_settings_local_are_skipped() {
        let other = obj(
            r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"bash /x/sinteractive-walltime-guard.sh"}]}]}}"#,
        );
        let mut s = Map::new();
        assert!(merge_hooks(&mut s, &snippet(), &other).unwrap());
        assert!(s["hooks"].get("UserPromptSubmit").is_none());
        assert!(s["hooks"].get("SessionStart").is_some());
    }

    #[test]
    fn malformed_hooks_are_refused() {
        let mut s = obj(r#"{"hooks":"nope"}"#);
        assert!(merge_hooks(&mut s, &snippet(), &Map::new()).is_err());
        let mut s = obj(r#"{"hooks":{"SessionStart":{}}}"#);
        assert!(merge_hooks(&mut s, &snippet(), &Map::new()).is_err());
        let mut s = obj(r#"{"hooks":null}"#);
        assert!(merge_hooks(&mut s, &snippet(), &Map::new()).unwrap());
    }

    const EXE: &str = "/opt/bin/sinteractive";

    #[test]
    fn renames_only_our_own_commands_onto_this_binary() {
        // The ungrouped verbs an earlier install wrote, by name or by path.
        assert_eq!(
            renamed_command("sinteractive hook prompt", EXE).as_deref(),
            Some("/opt/bin/sinteractive claude hook prompt")
        );
        assert_eq!(
            renamed_command("/usr/local/bin/sinteractive statusline", EXE).as_deref(),
            Some("/opt/bin/sinteractive claude statusline")
        );
        // The current verbs, but naming the binary by PATH or by another
        // path: pointed at this one.
        assert_eq!(
            renamed_command("sinteractive claude hook prompt", EXE).as_deref(),
            Some("/opt/bin/sinteractive claude hook prompt")
        );
        assert_eq!(
            renamed_command("/old/sinteractive  claude   statusline", EXE).as_deref(),
            Some("/opt/bin/sinteractive claude statusline")
        );
        // Already right, not ours, or carrying extra arguments: left alone.
        assert_eq!(
            renamed_command("/opt/bin/sinteractive claude hook prompt", EXE),
            None
        );
        assert_eq!(renamed_command("sinteractive status", EXE), None);
        assert_eq!(
            renamed_command("sinteractive hook prompt --json", EXE),
            None
        );
        assert_eq!(
            renamed_command("my-sinteractive-wrapper statusline", EXE),
            None
        );
        assert_eq!(renamed_command("my-sinteractive statusline", EXE), None);
        assert_eq!(
            renamed_command("bash ~/.claude/hooks/sinteractive-walltime-guard.sh", EXE),
            None
        );
    }

    #[test]
    fn migration_rewrites_an_earlier_install() {
        let mut s = obj(r#"{
            "hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"sinteractive hook session-start"}]}]},
            "statusLine":{"type":"command","command":"sinteractive claude statusline"}
        }"#);
        assert!(migrate_commands(&mut s, EXE));
        assert_eq!(
            s["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "/opt/bin/sinteractive claude hook session-start"
        );
        assert_eq!(
            s["statusLine"]["command"],
            "/opt/bin/sinteractive claude statusline"
        );
        // Idempotent, and a hand-written statusline is not ours to touch.
        assert!(!migrate_commands(&mut s, EXE));
        let mut mine = obj(r#"{"statusLine":{"type":"command","command":"my-prompt"}}"#);
        assert!(!migrate_commands(&mut mine, EXE));
    }

    #[test]
    fn statusline_only_when_absent() {
        let mut s = Map::new();
        assert!(merge_statusline(&mut s, EXE));
        assert_eq!(
            s["statusLine"]["command"],
            "/opt/bin/sinteractive claude statusline"
        );
        assert!(!merge_statusline(&mut s, EXE));
        let mut s = obj(r#"{"statusLine":{"type":"command","command":"mine"}}"#);
        assert!(!merge_statusline(&mut s, EXE));
        assert_eq!(s["statusLine"]["command"], "mine");
    }

    #[test]
    fn mcp_added_when_absent_and_repointed_when_ours() {
        let mut d = Map::new();
        assert_eq!(merge_mcp(&mut d, EXE).unwrap(), McpMerge::Added);
        assert_eq!(d["mcpServers"]["sinteractive"]["command"], EXE);
        assert_eq!(
            d["mcpServers"]["sinteractive"]["args"],
            json!(["claude", "mcp"])
        );
        assert_eq!(merge_mcp(&mut d, EXE).unwrap(), McpMerge::Unchanged);

        // What `claude mcp add` wrote before 1.0.1: the bare name, which
        // PATH may resolve to some other sinteractive. Its `env` survives.
        let mut d = obj(
            r#"{"mcpServers":{"sinteractive":{"type":"stdio","command":"sinteractive","args":["claude","mcp"],"env":{"A":"1"}}}}"#,
        );
        assert_eq!(merge_mcp(&mut d, EXE).unwrap(), McpMerge::Repointed);
        assert_eq!(d["mcpServers"]["sinteractive"]["command"], EXE);
        assert_eq!(d["mcpServers"]["sinteractive"]["env"]["A"], "1");
        let keys: Vec<&String> = d["mcpServers"]["sinteractive"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(keys, ["type", "command", "args", "env"]);

        // Another path and the pre-1.0 verb: both brought up to date.
        let mut d = obj(
            r#"{"mcpServers":{"sinteractive":{"command":"/old/sinteractive","args":["mcp"]}}}"#,
        );
        assert_eq!(merge_mcp(&mut d, EXE).unwrap(), McpMerge::Repointed);
        assert_eq!(d["mcpServers"]["sinteractive"]["command"], EXE);
        assert_eq!(
            d["mcpServers"]["sinteractive"]["args"],
            json!(["claude", "mcp"])
        );

        // Not ours: left alone.
        let mut d = obj(r#"{"mcpServers":{"sinteractive":{"command":"custom"}}}"#);
        assert_eq!(merge_mcp(&mut d, EXE).unwrap(), McpMerge::Unchanged);
        assert_eq!(d["mcpServers"]["sinteractive"]["command"], "custom");
        let mut d = obj(
            r#"{"mcpServers":{"sinteractive":{"command":"sinteractive","args":["claude","mcp","--verbose"]}}}"#,
        );
        assert_eq!(merge_mcp(&mut d, EXE).unwrap(), McpMerge::Unchanged);
        assert_eq!(d["mcpServers"]["sinteractive"]["command"], "sinteractive");

        let mut d = obj(r#"{"mcpServers":[]}"#);
        assert!(merge_mcp(&mut d, EXE).is_err());
    }

    #[test]
    fn load_object_rules() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        assert!(load_object(&p).unwrap().is_empty());
        fs::write(&p, "").unwrap();
        assert!(load_object(&p).unwrap().is_empty());
        fs::write(&p, "   \n").unwrap();
        assert!(load_object(&p).is_err());
        fs::write(&p, "[1]").unwrap();
        assert!(load_object(&p).is_err());
        fs::write(&p, "{\"a\":1,").unwrap();
        assert!(load_object(&p).is_err());
        fs::write(&p, "{\"a\":1}").unwrap();
        assert_eq!(load_object(&p).unwrap()["a"], 1);
    }

    #[test]
    fn write_json_backs_up_and_keeps_mode() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        assert_eq!(write_json(&p, &json!({"a": 1})).unwrap(), None);
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o644
        );
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let backup = write_json(&p, &json!({"a": 2})).unwrap().unwrap();
        assert!(backup
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("s.json.bak-"));
        assert_eq!(
            fs::read_to_string(&backup).unwrap().trim(),
            "{\n  \"a\": 1\n}"
        );
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load_object(&p).unwrap()["a"], 2);
    }
}
