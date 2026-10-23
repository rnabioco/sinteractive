//! `sinteractive session panes TARGET [--json]` — a session's terminal
//! panes, from the embedded zellij's `action list-panes --json --all` over
//! one ssh; and the `--pane ID|NAME|focused` resolution `peek` and `send`
//! share.
//!
//! A session is one tab with the user's shell in `terminal_0`; anything else
//! is a pane someone (the user, a plugin popup, `session exec`) opened. The
//! default stays `terminal_0` and costs no extra ssh: only a name or
//! `focused` needs the listing to resolve.

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::Value;

use super::common::{eprint_error, print_json, ssh_batch, Ctx, RunningSession};
use super::peek::first_stderr_line;
use crate::cli::PanesArgs;
use crate::zellij_cmd::ZellijEnv;

/// The pane `peek` and `send` act on when none is named: the shell zellij
/// starts every session with.
pub const DEFAULT_PANE: &str = "terminal_0";

/// One terminal pane, as `list_panes` reports it. Plugin panes (the status
/// bar and friends) are left out: there is nothing to peek at or type into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct PaneInfo {
    /// What `--pane` and the `pane` tool argument take, e.g. `terminal_3`.
    pub id: String,
    pub title: String,
    /// The command the pane runs (`bash -lc …`, or the shell's own).
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub focused: bool,
    pub floating: bool,
    /// The pane's command has exited and zellij is holding the pane open.
    pub exited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
    /// Name of the tab the pane is in.
    pub tab: String,
}

/// Terminal panes from `list-panes --json --all` output.
pub fn parse_panes(json: &str) -> Result<Vec<PaneInfo>> {
    let panes: Vec<Value> =
        serde_json::from_str(json).map_err(|e| anyhow!("unreadable pane list: {e}"))?;
    let text = |p: &Value, k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
    let flag = |p: &Value, k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    Ok(panes
        .iter()
        .filter(|p| !flag(p, "is_plugin"))
        .filter_map(|p| {
            Some(PaneInfo {
                id: format!("terminal_{}", p.get("id")?.as_u64()?),
                title: text(p, "title").unwrap_or_default(),
                command: text(p, "terminal_command").or_else(|| text(p, "pane_command")),
                cwd: text(p, "pane_cwd"),
                focused: flag(p, "is_focused"),
                floating: flag(p, "is_floating"),
                exited: flag(p, "exited"),
                exit_status: p
                    .get("exit_status")
                    .and_then(Value::as_i64)
                    .and_then(|n| i32::try_from(n).ok()),
                tab: text(p, "tab_name").unwrap_or_default(),
            })
        })
        .collect())
}

/// The session's terminal panes, over one ssh.
pub fn list_panes(ctx: &Ctx, session: &RunningSession) -> Result<Vec<PaneInfo>> {
    let env = ZellijEnv::new(&ctx.cfg, session.job_id)?;
    let remote = env
        .remote_argv(["action", "list-panes", "--json", "--all"])
        .join(" ");
    let out = ssh_batch(&session.node, 10, &remote).output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "could not list the panes of session {} on {}: {}",
            session.job_id,
            session.node,
            first_stderr_line(&out)
        ));
    }
    parse_panes(&String::from_utf8_lossy(&out.stdout))
}

/// `terminal_N` for an id (`terminal_N` or a bare `N`), else `None`.
fn pane_id(spec: &str) -> Option<String> {
    let n = spec.strip_prefix("terminal_").unwrap_or(spec);
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| format!("terminal_{n}"))
}

/// Pick the pane `spec` names out of `panes`: `focused` is the first focused
/// pane, anything else an id or an exact title.
pub fn select_pane(panes: &[PaneInfo], spec: &str) -> Result<String> {
    if spec == "focused" {
        return panes
            .iter()
            .find(|p| p.focused)
            .map(|p| p.id.clone())
            .ok_or_else(|| anyhow!("no pane is focused"));
    }
    let named: Vec<&PaneInfo> = panes.iter().filter(|p| p.title == spec).collect();
    match named.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(anyhow!(
            "no pane named '{spec}' (panes: {})",
            panes
                .iter()
                .map(|p| format!("{} '{}'", p.id, p.title))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        many => Err(anyhow!(
            "{} panes are named '{spec}' ({}); use an id",
            many.len(),
            many.iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The zellij pane id `--pane SPEC` means. `None` is [`DEFAULT_PANE`]; an id
/// passes straight through; a name or `focused` is looked up with one more
/// ssh.
pub fn resolve_pane(ctx: &Ctx, session: &RunningSession, spec: Option<&str>) -> Result<String> {
    let Some(spec) = spec.map(str::trim) else {
        return Ok(DEFAULT_PANE.to_string());
    };
    if let Some(id) = pane_id(spec) {
        return Ok(id);
    }
    if spec.is_empty() {
        return Err(anyhow!("pane is empty"));
    }
    select_pane(&list_panes(ctx, session)?, spec)
}

pub fn run(args: PanesArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let Some(session) = ctx.resolve_running(&args.target)? else {
        return Ok(1);
    };
    let panes = match list_panes(&ctx, &session) {
        Ok(panes) => panes,
        Err(e) => {
            eprint_error(&ctx.palette(2), &format!("{e:#}"));
            return Ok(1);
        }
    };
    if args.json {
        print_json(&serde_json::json!({
            "job_id": session.job_id,
            "node": session.node,
            "panes": panes,
        }))?;
        return Ok(0);
    }
    for p in &panes {
        let mut flags = Vec::new();
        if p.focused {
            flags.push("focused".to_string());
        }
        if p.floating {
            flags.push("floating".to_string());
        }
        if p.exited {
            flags.push(match p.exit_status {
                Some(n) => format!("exited {n}"),
                None => "exited".to_string(),
            });
        }
        println!(
            "{}\t{}\t{}\t{}\t{}",
            p.id,
            p.title,
            p.tab,
            p.command.as_deref().unwrap_or("-"),
            flags.join(",")
        );
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a real `zellij action list-panes --json --all` (0.45.1).
    pub const LISTING: &str = r#"[
      {"id":0,"is_plugin":true,"is_focused":false,"is_floating":false,"title":"tab-bar","exited":false,"exit_status":null,"tab_name":"Tab #1"},
      {"id":0,"is_plugin":false,"is_focused":true,"is_floating":false,"title":"Pane #1","exited":false,"exit_status":null,"terminal_command":null,"tab_name":"Tab #1","pane_command":"/bin/bash","pane_cwd":"/tmp/jhessel"},
      {"id":1,"is_plugin":false,"is_focused":false,"is_floating":true,"title":"agent-1","exited":true,"exit_status":3,"terminal_command":"bash -lc exit 3","tab_name":"Tab #1"}
    ]"#;

    #[test]
    fn parses_terminal_panes_only() {
        let panes = parse_panes(LISTING).unwrap();
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].id, "terminal_0");
        assert_eq!(panes[0].title, "Pane #1");
        assert_eq!(panes[0].command.as_deref(), Some("/bin/bash"));
        assert_eq!(panes[0].cwd.as_deref(), Some("/tmp/jhessel"));
        assert!(panes[0].focused && !panes[0].floating && !panes[0].exited);
        assert_eq!(panes[0].exit_status, None);
        assert_eq!(panes[0].tab, "Tab #1");
        assert_eq!(panes[1].id, "terminal_1");
        assert_eq!(panes[1].command.as_deref(), Some("bash -lc exit 3"));
        assert!(panes[1].floating && panes[1].exited);
        assert_eq!(panes[1].exit_status, Some(3));
        assert!(parse_panes("not json").is_err());
    }

    #[test]
    fn serialises_optional_fields_only_when_known() {
        let panes = parse_panes(LISTING).unwrap();
        let a = serde_json::to_value(&panes[0]).unwrap();
        assert!(a.get("exit_status").is_none());
        assert_eq!(a["cwd"], "/tmp/jhessel");
        let b = serde_json::to_value(&panes[1]).unwrap();
        assert!(b.get("cwd").is_none());
        assert_eq!(b["exit_status"], 3);
    }

    #[test]
    fn ids_names_and_focused_select_panes() {
        assert_eq!(pane_id("terminal_7").as_deref(), Some("terminal_7"));
        assert_eq!(pane_id("7").as_deref(), Some("terminal_7"));
        assert_eq!(pane_id("agent-1"), None);
        assert_eq!(pane_id("terminal_"), None);
        assert_eq!(pane_id(""), None);

        let panes = parse_panes(LISTING).unwrap();
        assert_eq!(select_pane(&panes, "focused").unwrap(), "terminal_0");
        assert_eq!(select_pane(&panes, "agent-1").unwrap(), "terminal_1");
        let err = select_pane(&panes, "nope").unwrap_err().to_string();
        assert!(err.contains("no pane named 'nope'") && err.contains("terminal_1"));

        let mut twice = panes.clone();
        twice[0].title = "agent-1".into();
        let err = select_pane(&twice, "agent-1").unwrap_err().to_string();
        assert!(err.contains("2 panes are named"), "{err}");
        twice.iter_mut().for_each(|p| p.focused = false);
        assert!(select_pane(&twice, "focused").is_err());
    }
}
