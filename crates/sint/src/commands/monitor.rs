//! `sinteractive monitor [TARGET|HOST] [--live] [--json]` — watch a
//! session's node, nvitop-style.
//!
//! Where the numbers come from:
//! - a session (job id, name, or the current one): the snapshot its
//!   in-session sampler writes to `<cache>/<jobid>.metrics.json` every few
//!   seconds. No ssh, works from the login node; a missing or stale file
//!   (> 30 s) is "no snapshot yet", and the TUI keeps polling.
//! - `--live`, or a bare hostname: `ssh -o BatchMode=yes HOST <exe>
//!   snapshot --json` every 2 s (run locally when HOST is this machine).
//!
//! `--json` prints the latest snapshot once. Without a tty the human dump
//! is printed once instead of the TUI.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use sint_core::metrics::{self, Snapshot};
use sint_core::session::{parse_comment, Target};
use sint_core::slurm::SlurmError;
use sint_core::state::StateDir;
use sint_core::theme::{is_tty, Theme};

use super::common::{current_exe, eprint_error, print_json, Ctx};
use super::monitor_tui;
use super::snapshot::render_human;
use crate::cli::MonitorArgs;
use crate::zellij_cmd::shell_quote;

/// How often the TUI refreshes its source.
pub const REFRESH: Duration = Duration::from_secs(2);

/// Where snapshots come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `<cache>/<jobid>.metrics.json`, written by the session's sampler.
    Cache { job_id: u64 },
    /// `snapshot --json` on `host`, over ssh unless `local`.
    Live { host: String, local: bool },
}

/// What the header names.
#[derive(Debug, Clone, Default)]
pub struct Label {
    pub host: String,
    pub job_id: Option<u64>,
    pub job_name: Option<String>,
}

/// One delivery from a source.
pub enum Msg {
    Snapshot(Box<Snapshot>),
    /// The source produced nothing usable; shown until the next snapshot.
    Waiting(String),
}

/// The "no snapshot yet" wording, shared by `--json` and the TUI.
pub fn waiting_message(job_id: u64, snap: Option<&Snapshot>, now: i64) -> String {
    match snap {
        Some(s) => format!(
            "snapshot for job {job_id} is {}s old — the session's sampler writes one every 5 s",
            s.age_secs(now)
        ),
        None => {
            format!("no snapshot yet for job {job_id} — the session's sampler writes one every 5 s")
        }
    }
}

/// Read the cache once: a fresh snapshot, or the waiting message.
pub fn poll_cache(state: &StateDir, job_id: u64) -> Msg {
    let now = sint_core::now_epoch();
    match metrics::read_snapshot(state, job_id) {
        Some(s) if !s.is_stale(now) => Msg::Snapshot(Box::new(s)),
        other => Msg::Waiting(waiting_message(job_id, other.as_ref(), now)),
    }
}

/// The argv that runs `snapshot --json` on `host` (or here).
pub fn live_command(host: &str, local: bool) -> Result<Command> {
    let exe = current_exe()?;
    if local {
        let mut c = Command::new(exe);
        c.args(["snapshot", "--json"]);
        return Ok(c);
    }
    let remote = format!("{} snapshot --json", shell_quote(&exe.to_string_lossy()));
    let mut c = Command::new("ssh");
    c.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=5",
        "-o",
        "LogLevel=ERROR",
        host,
        &remote,
    ]);
    Ok(c)
}

/// Run `snapshot --json` on the host once.
pub fn poll_live(host: &str, local: bool) -> Msg {
    let mut cmd = match live_command(host, local) {
        Ok(c) => c,
        Err(e) => return Msg::Waiting(e.to_string()),
    };
    let out = cmd
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output();
    match out {
        Ok(o) if o.status.success() => match serde_json::from_slice::<Snapshot>(&o.stdout) {
            Ok(s) => Msg::Snapshot(Box::new(s)),
            Err(e) => Msg::Waiting(format!("bad snapshot from {host}: {e}")),
        },
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let err = err.trim();
            Msg::Waiting(if err.is_empty() {
                format!("snapshot on {host} failed ({})", o.status)
            } else {
                format!("{host}: {err}")
            })
        }
        Err(e) => Msg::Waiting(format!("cannot run ssh: {e}")),
    }
}

fn poll(source: &Source, state: &StateDir) -> Msg {
    match source {
        Source::Cache { job_id } => poll_cache(state, *job_id),
        Source::Live { host, local } => poll_live(host, *local),
    }
}

/// Whether `host` names this machine.
pub fn is_local_host(host: &str) -> bool {
    let short = host.split('.').next().unwrap_or(host);
    host == "localhost" || short == metrics::hostname()
}

pub fn run(args: MonitorArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let p = ctx.palette(2);

    // Resolve the target to a source and a label.
    let (mut source, mut label) = match args.target.as_deref() {
        None => match ctx.cfg.job_id {
            Some(id) => (
                Source::Cache { job_id: id },
                Label {
                    job_id: Some(id),
                    job_name: ctx.cfg.name.clone(),
                    ..Default::default()
                },
            ),
            None => {
                eprint_error(
                    &p,
                    "monitor requires a JOBID, NAME or hostname outside a session.",
                );
                eprintln!(
                    "{}Run 'sinteractive list' to see available sessions.{}",
                    p.dim, p.reset
                );
                return Ok(1);
            }
        },
        Some(t) => match Target::parse(t) {
            Target::JobId(id) => (
                Source::Cache { job_id: id },
                Label {
                    job_id: Some(id),
                    ..Default::default()
                },
            ),
            Target::Name(name) => match ctx.resolve(Some(&name)) {
                Ok(id) => (
                    Source::Cache { job_id: id },
                    Label {
                        job_id: Some(id),
                        job_name: Some(name),
                        ..Default::default()
                    },
                ),
                // A Slurm failure is still fatal; anything else means "not
                // a session name", so the word is a host.
                Err(e) if e.downcast_ref::<SlurmError>().is_some() => return Err(e),
                Err(_) => (
                    Source::Live {
                        host: name.clone(),
                        local: is_local_host(&name),
                    },
                    Label {
                        host: name,
                        ..Default::default()
                    },
                ),
            },
        },
    };

    // `--live` on a session: find its node.
    if args.live {
        if let Source::Cache { job_id } = source {
            let Some(row) = ctx.slurm.job(job_id)? else {
                eprint_error(
                    &p,
                    &format!("job {job_id} not found (finished or cancelled)"),
                );
                return Ok(1);
            };
            if row.node.is_empty() {
                eprint_error(&p, &format!("job {job_id} has no node yet ({})", row.state));
                return Ok(1);
            }
            let host = row.node.split(',').next().unwrap_or(&row.node).to_string();
            if label.job_name.is_none() {
                label.job_name = parse_comment(&row.comment).flatten();
            }
            label.host = host.clone();
            source = Source::Live {
                local: is_local_host(&host),
                host,
            };
        }
    }

    if args.json {
        return match poll(&source, &ctx.state) {
            Msg::Snapshot(s) => {
                print_json(&s)?;
                Ok(0)
            }
            Msg::Waiting(msg) => {
                eprint_error(&p, &msg);
                Ok(1)
            }
        };
    }

    // No tty: the human dump once, like `snapshot`.
    if !is_tty(1) {
        return match poll(&source, &ctx.state) {
            Msg::Snapshot(s) => {
                print!("{}", render_human(&s, None, &ctx.palette(1)));
                Ok(0)
            }
            Msg::Waiting(msg) => {
                eprint_error(&p, &msg);
                Ok(1)
            }
        };
    }

    // The TUI: a feeder thread polls the source, the UI thread draws.
    let theme = Theme::detect(1);
    let (tx, rx) = mpsc::channel::<Msg>();
    let feeder_source = source.clone();
    let state = ctx.state.clone();
    std::thread::spawn(move || loop {
        let msg = poll(&feeder_source, &state);
        if tx.send(msg).is_err() {
            return;
        }
        // `poll_live` already spends ~1 s inside `snapshot`; keep the
        // cadence at REFRESH either way.
        std::thread::sleep(match feeder_source {
            Source::Live { .. } => REFRESH.saturating_sub(super::snapshot::SAMPLE_GAP),
            Source::Cache { .. } => REFRESH,
        });
    });
    let mode = match &source {
        Source::Cache { .. } => "cache",
        Source::Live { local: true, .. } => "local",
        Source::Live { local: false, .. } => "ssh",
    };
    monitor_tui::run(rx, label, mode, theme).map_err(|e| anyhow!("monitor: {e}"))?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_wording() {
        assert_eq!(
            waiting_message(7, None, 100),
            "no snapshot yet for job 7 — the session's sampler writes one every 5 s"
        );
        let s = Snapshot {
            ts: 40,
            ..Default::default()
        };
        assert_eq!(
            waiting_message(7, Some(&s), 100),
            "snapshot for job 7 is 60s old — the session's sampler writes one every 5 s"
        );
    }

    #[test]
    fn cache_poll_distinguishes_fresh_stale_missing() {
        let dir = tempfile::tempdir().unwrap();
        let sd = StateDir(dir.path().to_path_buf());
        assert!(matches!(poll_cache(&sd, 1), Msg::Waiting(m) if m.starts_with("no snapshot yet")));
        let now = sint_core::now_epoch();
        let mut s = Snapshot {
            host: "n1".into(),
            ts: now,
            ..Default::default()
        };
        metrics::write_snapshot(&sd, 1, &s).unwrap();
        assert!(matches!(poll_cache(&sd, 1), Msg::Snapshot(got) if got.host == "n1"));
        s.ts = now - 31;
        metrics::write_snapshot(&sd, 1, &s).unwrap();
        assert!(matches!(poll_cache(&sd, 1), Msg::Waiting(m) if m.contains("31s old")));
    }

    #[test]
    fn live_command_shapes() {
        let c = live_command("node01", false).unwrap();
        assert_eq!(c.get_program(), "ssh");
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(&args[..2], ["-o", "BatchMode=yes"]);
        assert_eq!(args[args.len() - 2], "node01");
        assert!(args.last().unwrap().ends_with(" snapshot --json"));

        let c = live_command("here", true).unwrap();
        assert_ne!(c.get_program(), "ssh");
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, ["snapshot", "--json"]);
    }

    #[test]
    fn local_host_detection() {
        assert!(is_local_host("localhost"));
        assert!(is_local_host(&metrics::hostname()));
        assert!(is_local_host(&format!(
            "{}.example.edu",
            metrics::hostname()
        )));
        assert!(!is_local_host("definitely-not-this-host"));
    }
}
