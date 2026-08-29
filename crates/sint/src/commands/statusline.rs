//! `sinteractive statusline` — a Claude Code `statusLine` command.
//!
//! Claude Code runs the configured command on every message (and every
//! `refreshInterval` seconds), passing a JSON object on stdin describing the
//! session (`model.display_name`, `context_window.used_percentage`,
//! `workspace.current_dir`, …), and shows the command's first line of stdout
//! under its input box. ANSI colour is honoured.
//!
//! The line: `⏺ Opus · ctx 42% · ~/d/r/sinteractive`, on a login node and
//! inside a session alike. The job, the walltime left and the notice count
//! are the status bar's: it is on screen the whole time, in every pane, and
//! says all three in more detail than this line had room for. Printing them
//! twice only crowded out the working directory.
//!
//! `install-claude` registers it as
//! `{"statusLine":{"type":"command","command":"sinteractive statusline","refreshInterval":5}}`.

use std::io::Read;

use anyhow::Result;
use serde_json::Value;
use sint_core::color::Palette;
use sint_core::config::ColorMode;

/// How wide the working directory may be before it is shortened.
const CWD_WIDTH: usize = 36;

/// Fields we read from Claude's payload; everything is optional because the
/// schema grows over time.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ClaudeStatus {
    pub model: Option<String>,
    pub context_pct: Option<f64>,
    pub cwd: Option<String>,
    pub cost_usd: Option<f64>,
}

pub fn parse_claude_status(json: &str) -> ClaudeStatus {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let model = v
        .pointer("/model/display_name")
        .or_else(|| v.pointer("/model/id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let context_pct = v
        .pointer("/context_window/used_percentage")
        .and_then(Value::as_f64);
    let cwd = v
        .pointer("/workspace/current_dir")
        .or_else(|| v.pointer("/cwd"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let cost_usd = v.pointer("/cost/total_cost_usd").and_then(Value::as_f64);
    ClaudeStatus {
        model,
        context_pct,
        cwd,
        cost_usd,
    }
}

/// Abbreviate `$HOME` to `~`.
fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && path.starts_with(&h) => format!("~{}", &path[h.len()..]),
        _ => path.to_string(),
    }
}

/// The working directory, narrow enough that a deep tree cannot push the
/// rest of the line off the terminal.
///
/// Past `budget` columns every directory above the last falls back to its
/// initial — `~/d/r/s/.c/w/fix-session-cache-dir` — which bounds the width
/// at two columns per level however deep the tree goes. Still too wide, and
/// leading levels are dropped for a `…`; a last component wider than the
/// budget on its own keeps its tail, which is the half that tells two long
/// sibling directories apart.
fn short_path(path: &str, budget: usize) -> String {
    let full = tilde(path);
    if full.chars().count() <= budget {
        return full;
    }
    let mut parts: Vec<&str> = full.split('/').collect();
    let leaf = parts.pop().unwrap_or_default();
    // A dotted directory keeps its dot: `.c` is legible where `.` is not.
    let mut initials: Vec<String> = parts
        .iter()
        .map(|p| {
            p.chars()
                .take(if p.starts_with('.') { 2 } else { 1 })
                .collect()
        })
        .collect();
    let mut elided = false;
    loop {
        let mut line = String::new();
        if elided {
            line.push_str("…/");
        }
        for i in &initials {
            line.push_str(i);
            line.push('/');
        }
        line.push_str(leaf);
        if line.chars().count() <= budget {
            return line;
        }
        if initials.is_empty() {
            let n = leaf.chars().count();
            let tail: String = leaf.chars().skip(n + 1 - budget).collect();
            return format!("…{tail}");
        }
        initials.remove(0);
        elided = true;
    }
}

pub fn render(claude: &ClaudeStatus, p: &Palette) -> String {
    let sep = format!(" {}·{} ", p.dim, p.reset);
    let mut parts: Vec<String> = Vec::new();
    let model = claude.model.as_deref().unwrap_or("claude");
    parts.push(format!("{}⏺{} {model}", p.id, p.reset));
    if let Some(pct) = claude.context_pct {
        let col = if pct >= 90.0 {
            &p.err
        } else if pct >= 70.0 {
            &p.warn
        } else {
            &p.dim
        };
        parts.push(format!("{col}ctx {pct:.0}%{}", p.reset));
    }
    if let Some(cwd) = &claude.cwd {
        parts.push(format!(
            "{}{}{}",
            p.dim,
            short_path(cwd, CWD_WIDTH),
            p.reset
        ));
    }
    parts.join(&sep)
}

pub fn run() -> Result<i32> {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let claude = parse_claude_status(&input);
    // Claude renders ANSI, but stdout is a pipe: force colour unless the
    // user turned it off.
    let mode = match ColorMode::from_env() {
        ColorMode::Never => ColorMode::Never,
        _ => ColorMode::Always,
    };
    let p = Palette::for_fd(mode, 1);
    println!("{}", render(&claude, &p));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_payload() {
        let c = parse_claude_status(
            r#"{"model":{"id":"claude-opus-5","display_name":"Opus"},"context_window":{"used_percentage":42.4},"workspace":{"current_dir":"/x/y"},"cost":{"total_cost_usd":0.12}}"#,
        );
        assert_eq!(c.model.as_deref(), Some("Opus"));
        assert_eq!(c.context_pct, Some(42.4));
        assert_eq!(c.cwd.as_deref(), Some("/x/y"));
        assert_eq!(c.cost_usd, Some(0.12));
        assert_eq!(parse_claude_status("not json"), ClaudeStatus::default());
    }

    #[test]
    fn renders_the_claude_side_only() {
        let p = Palette::none();
        let c = ClaudeStatus {
            model: Some("Opus".into()),
            context_pct: Some(42.0),
            cwd: Some("/proj".into()),
            cost_usd: None,
        };
        assert_eq!(render(&c, &p), "⏺ Opus · ctx 42% · /proj");
        assert_eq!(
            render(&ClaudeStatus::default(), &p),
            "⏺ claude",
            "an empty payload is still a line"
        );
    }

    #[test]
    fn a_deep_working_directory_stops_growing() {
        // Short enough to leave alone.
        assert_eq!(short_path("~/proj", 36), "~/proj");
        assert_eq!(
            short_path("~/devel/rnabioco/sinteractive", 36),
            "~/devel/rnabioco/sinteractive"
        );
        // Past the budget the ancestors go to initials, dots kept.
        assert_eq!(
            short_path(
                "~/devel/rnabioco/sinteractive/.claude/worktrees/fix-session-cache-dir",
                36
            ),
            "~/d/r/s/.c/w/fix-session-cache-dir"
        );
        // An absolute path keeps its leading slash.
        assert_eq!(
            short_path("/opt/a/bb/ccc/dddd/eeeee/ffffff/ggggggg/hhhh/crates", 36),
            "/o/a/b/c/d/e/f/g/h/crates"
        );
        // Deeper still: levels drop from the left for a `…`.
        assert_eq!(
            short_path("~/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t/u/leaf", 20),
            "…/o/p/q/r/s/t/u/leaf"
        );
        // A leaf wider than the whole budget keeps its tail: the half that
        // tells two long sibling directories apart.
        assert_eq!(
            short_path("~/x/some-very-long-directory-name-indeed", 20),
            "…rectory-name-indeed"
        );
    }
}
