//! `sinteractive statusline` — a Claude Code `statusLine` command.
//!
//! Claude Code runs the configured command on every message (and every
//! `refreshInterval` seconds), passing a JSON object on stdin describing the
//! session (`model.display_name`, `context_window.used_percentage`,
//! `workspace.current_dir`, …), and shows the command's first line of stdout
//! under its input box. ANSI colour is honoured.
//!
//! The line: `⏺ Opus · ctx 42% · ~/proj` on a login node, and inside an
//! sinteractive session additionally `· sint 147845 mywork · 2h41m · ⚠1`.
//! Everything session-side comes from the cache files only — the state file
//! aged exactly and the notices file — so a 5-second refresh never touches
//! the scheduler.
//!
//! `install-claude` registers it as
//! `{"statusLine":{"type":"command","command":"sinteractive statusline","refreshInterval":5}}`.

use std::io::Read;

use anyhow::Result;
use serde_json::Value;
use sint_core::color::Palette;
use sint_core::config::ColorMode;
use sint_core::notices;
use sint_core::now_epoch;
use sint_core::time::format_short_duration;

use super::common::Ctx;

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

/// Session facts for the line, from the cache only.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SessionBits {
    pub job_id: u64,
    pub name: Option<String>,
    pub remaining: Option<i64>,
    pub notices: usize,
    pub severe: bool,
}

/// Abbreviate `$HOME` to `~`.
fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && path.starts_with(&h) => format!("~{}", &path[h.len()..]),
        _ => path.to_string(),
    }
}

pub fn render(claude: &ClaudeStatus, session: Option<&SessionBits>, p: &Palette) -> String {
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
        parts.push(format!("{}{}{}", p.dim, tilde(cwd), p.reset));
    }
    if let Some(s) = session {
        let mut id = format!("{}sint{} {}{}{}", p.dim, p.reset, p.id, s.job_id, p.reset);
        if let Some(n) = &s.name {
            id.push_str(&format!(" {}{n}{}", p.id, p.reset));
        }
        parts.push(id);
        match s.remaining {
            Some(rem) => {
                let col = if rem <= 600 {
                    &p.err
                } else if rem <= 3600 {
                    &p.warn
                } else {
                    &p.ok
                };
                parts.push(format!("{col}{}{}", format_short_duration(rem), p.reset));
            }
            None => parts.push(format!("{}budget stale{}", p.warn, p.reset)),
        }
        if s.notices > 0 {
            let col = if s.severe { &p.err } else { &p.warn };
            parts.push(format!("{col}⚠{}{}", s.notices, p.reset));
        }
    }
    parts.join(&sep)
}

pub fn run() -> Result<i32> {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let claude = parse_claude_status(&input);
    let ctx = Ctx::new();
    let session = ctx.cfg.job_id.map(|job_id| {
        let now = now_epoch();
        let state = ctx.state.read_state(job_id);
        let notes = notices::read(&ctx.state, job_id);
        SessionBits {
            job_id,
            name: state
                .as_ref()
                .and_then(|s| s.name.clone())
                .or_else(|| ctx.cfg.name.clone()),
            remaining: state.as_ref().and_then(|s| s.aged_remaining(now)),
            severe: notes.iter().any(|n| n.is_severe()),
            notices: notes.len(),
        }
    });
    // Claude renders ANSI, but stdout is a pipe: force colour unless the
    // user turned it off.
    let mode = match ColorMode::from_env() {
        ColorMode::Never => ColorMode::Never,
        _ => ColorMode::Always,
    };
    let p = Palette::for_fd(mode, 1);
    println!("{}", render(&claude, session.as_ref(), &p));
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
    fn renders_login_and_session_lines() {
        let p = Palette::none();
        let c = ClaudeStatus {
            model: Some("Opus".into()),
            context_pct: Some(42.0),
            cwd: Some("/proj".into()),
            cost_usd: None,
        };
        assert_eq!(render(&c, None, &p), "⏺ Opus · ctx 42% · /proj");
        let s = SessionBits {
            job_id: 147845,
            name: Some("mywork".into()),
            remaining: Some(9660),
            notices: 1,
            severe: true,
        };
        assert_eq!(
            render(&c, Some(&s), &p),
            "⏺ Opus · ctx 42% · /proj · sint 147845 mywork · 2h 41m · ⚠1"
        );
        let stale = SessionBits {
            remaining: None,
            notices: 0,
            ..s
        };
        assert!(render(&c, Some(&stale), &p).ends_with("budget stale"));
    }
}
