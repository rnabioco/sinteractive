//! `sinteractive __job` — the batch job body, running on the compute node
//! under sbatch.
//!
//! Ports `start_tmux` (script lines 2750-2974) and `status_dot_loop`
//! (2457-2748) onto the embedded zellij:
//!
//! 1. bring up a headless zellij server for this job (`attach
//!    --create-background`) with the `SLURM_*` variables stripped from its
//!    environment and `SINTERACTIVE_JOB_ID`/`SINTERACTIVE_NAME` exported, so
//!    every shell in the session sees the job identity but no tool inside it
//!    thinks it is a job step;
//! 2. write the readiness marker the launcher polls for;
//! 3. run the status loop until the session ends or Slurm signals us.
//!
//! The loop itself is [`JobLoop`]: pure tick logic over `now` and a
//! [`Deps`] trait for every side effect, so its timing rules can be tested
//! without zellij or Slurm. The driver ([`run`]) is the thin I/O half.
//!
//! What the 0.x loop did that has no zellij equivalent:
//! - the terminal bell (`\a` into every pane tty) as the final countdown
//!   starts — zellij owns the ptys and has no "ring the bell" action; the
//!   red spinner in the status bar is the cue;
//! - the tmux `status-left`/`status-right` strings — replaced by one
//!   [`StatusMsg`] piped to the status plugin, which renders it.
//!
//! Undocumented knob for the tests: `SINTERACTIVE_POLL_FAST=<secs>` caps
//! every wait in this file (server readiness poll, loop tick, pre-kill
//! pause).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use sint_core::config::Config;
use sint_core::notices::{self, Notice};
use sint_core::now_epoch;
use sint_core::quota::{self, QuotaSnapshot};
use sint_core::slurm::squeue::JobRow;
use sint_core::state::StateFile;
use sint_core::time::{format_short_duration, slurm_timestamp_to_epoch};
use sint_proto::{Severity, StatusMsg, PIPE_NAME};

use super::common::Ctx;
use crate::bundle;
use crate::cli::JobArgs;
use crate::zellij_cmd::{self, ZellijEnv};

/// Floor on scheduler queries however often the loop wakes, and the window
/// in which a confirmed end time still counts as fresh enough to write.
pub const END_MIN_GAP: i64 = 5;
/// How often the `2R 1PD` other-jobs summary is refreshed.
pub const JOBS_EVERY: i64 = 30;
/// How often `list-sessions` is consulted to see whether the session is up.
pub const ALIVE_EVERY: i64 = 2;
/// Consecutive alive-check misses before the session counts as gone.
const ALIVE_MISSES: u32 = 3;
/// The line typed into the shell as the session is ended for walltime.
pub const ENDING_LINE: &str = "[sinteractive] walltime reached — ending session";

/// Key legend pages for the bar's help mode.
pub const HELP_PAGES: &[&[(&str, &str)]] = &[
    &[
        ("n", "notices"),
        ("h", "help"),
        ("m", "monitor"),
        ("q", "queue"),
        ("d", "detach"),
        ("$", "rename"),
    ],
    &[
        ("c", "new pane"),
        ("\"", "split down"),
        ("%", "split right"),
        ("x", "close"),
        ("z", "zoom"),
        ("o", "next pane"),
        ("←↑→↓", "focus"),
        ("[", "scroll"),
        ("r", "resize"),
    ],
];

fn help_pages() -> Vec<Vec<(String, String)>> {
    HELP_PAGES
        .iter()
        .map(|page| {
            page.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .collect()
}

// ---- the pure loop ---------------------------------------------------------

/// Side effects the loop needs, so the tick logic can run against a fake.
pub trait Deps {
    /// Ask Slurm for this job's end time; `Some` only when confirmed.
    fn query_end_epoch(&mut self) -> Option<i64>;
    /// Consume a pending `<jobid>.poke` (true when one was there).
    fn take_poke(&mut self) -> bool;
    /// The current quota snapshot (cache, re-probed when stale); `None`
    /// when the cluster has no quota daemons or nothing answered.
    fn read_quota(&mut self) -> Option<QuotaSnapshot>;
    /// Whether the Claude Code install hint is due (a `claude` process is
    /// running for this user and the integration is not installed).
    fn claude_hint_wanted(&mut self) -> bool;
    /// The user's other RUNNING/PENDING jobs as `2R 1PD` (empty when none).
    fn other_jobs(&mut self) -> String;
    /// Whether zellij still lists the session.
    fn session_alive(&mut self) -> bool;
    /// Write `<jobid>.json`.
    fn write_state(&mut self, state: &StateFile);
    /// Write (or remove, when empty) `<jobid>.notices`.
    fn write_notices(&mut self, notices: &[Notice]);
    /// Pipe a message to the status plugin.
    fn send_status(&mut self, msg: &StatusMsg);
}

/// What the driver does after a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Sleep this long, then tick again.
    Continue(Duration),
    /// Walltime reached: announce, kill the session, stop.
    Ending,
    /// The session is no longer listed: stop.
    Gone,
}

/// Fixed-for-the-session inputs of the loop.
#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub job_id: u64,
    pub name: Option<String>,
    pub host: String,
    pub warn_yellow: i64,
    pub warn_red: i64,
    pub grace: i64,
    pub poll: i64,
    pub quota_poll: i64,
    /// `(reservation, ends_epoch)` from `--maint=NAME@EPOCH`.
    pub maint: Option<(String, i64)>,
}

impl LoopConfig {
    pub fn from_config(cfg: &Config, job_id: u64, name: Option<String>, host: String) -> Self {
        LoopConfig {
            job_id,
            name,
            host,
            warn_yellow: cfg.warn_yellow,
            warn_red: cfg.warn_red,
            grace: cfg.grace,
            poll: cfg.poll,
            quota_poll: cfg.quota_poll,
            maint: None,
        }
    }
}

/// `NAME@EPOCH` → `(NAME, EPOCH)`; the name may itself contain `@`.
pub fn parse_maint(spec: &str) -> Option<(String, i64)> {
    let (name, epoch) = spec.rsplit_once('@')?;
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), epoch.trim().parse().ok()?))
}

/// The status loop's state between ticks. See the module docs.
#[derive(Debug)]
pub struct JobLoop {
    cfg: LoopConfig,
    /// Last end time Slurm confirmed; the countdown keeps running from it
    /// while the scheduler is unreachable.
    end_epoch: Option<i64>,
    /// When the deadline was last asked for / last confirmed.
    end_query: i64,
    end_checked: i64,
    /// When `<jobid>.json` was last written (0 = a write is due).
    state_written: i64,
    quota_checked: i64,
    quota_notice: Option<Notice>,
    hint_checked: i64,
    hint_wanted: bool,
    maint_notice: Option<Notice>,
    last_notices: Option<Vec<Notice>>,
    jobs_checked: i64,
    jobs: String,
    alive_checked: Option<i64>,
    /// Consecutive failed alive checks.
    alive_misses: u32,
    /// Second in which the last status message went out.
    last_sent: Option<i64>,
    /// Whether the red phase has been entered (0.x `belled`): the deadline
    /// is re-confirmed once on entry before the spinner starts.
    in_red: bool,
}

impl JobLoop {
    pub fn new(cfg: LoopConfig) -> Self {
        let maint_notice = cfg
            .maint
            .as_ref()
            .map(|(name, ends)| notices::maint_notice(*ends, name));
        JobLoop {
            cfg,
            end_epoch: None,
            end_query: 0,
            end_checked: 0,
            state_written: 0,
            quota_checked: 0,
            quota_notice: None,
            hint_checked: 0,
            hint_wanted: false,
            maint_notice,
            last_notices: None,
            jobs_checked: 0,
            jobs: String::new(),
            alive_checked: None,
            alive_misses: 0,
            last_sent: None,
            in_red: false,
        }
    }

    /// Ask Slurm now, bypassing the rate floor. Returns the confirmed end
    /// time; a failure leaves the previous one alone.
    fn refresh_end_epoch(&mut self, now: i64, deps: &mut dyn Deps) -> Option<i64> {
        self.end_query = now;
        let e = deps.query_end_epoch()?;
        self.end_epoch = Some(e);
        self.end_checked = now;
        Some(e)
    }

    /// The notices in display order: quota (severe) first, then the
    /// maintenance trim, then the Claude Code hint.
    fn notices(&self) -> Vec<Notice> {
        let mut v = Vec::new();
        if let Some(q) = &self.quota_notice {
            v.push(q.clone());
        }
        if let Some(m) = &self.maint_notice {
            v.push(m.clone());
        }
        if self.hint_wanted {
            v.push(notices::claude_hint_notice());
        }
        v
    }

    fn status_msg(&self, now: i64, severity: Severity, remaining: Option<i64>) -> StatusMsg {
        let remaining_text = match (severity, remaining) {
            (_, None) => String::new(),
            (Severity::Red | Severity::Ending, Some(r)) => format!("{}:{:02}", r / 60, r % 60),
            (_, Some(r)) => format_short_duration(r),
        };
        StatusMsg {
            job_id: self.cfg.job_id,
            name: self.cfg.name.clone(),
            host: self.cfg.host.clone(),
            severity,
            remaining: remaining_text,
            remaining_secs: remaining,
            load: String::new(),
            gpu: String::new(),
            jobs: self.jobs.clone(),
            notices: self
                .notices()
                .into_iter()
                .map(|n| sint_proto::Notice {
                    kind: n.kind,
                    text: n.text,
                })
                .collect(),
            help: help_pages(),
            hosts: Vec::new(),
            sent_epoch: now,
        }
    }

    /// Pipe at most once per wall-clock second: the plugin animates its
    /// own spinner and counts down between messages.
    fn send(&mut self, now: i64, deps: &mut dyn Deps, severity: Severity, remaining: Option<i64>) {
        if self.last_sent == Some(now) {
            return;
        }
        self.last_sent = Some(now);
        let msg = self.status_msg(now, severity, remaining);
        deps.send_status(&msg);
    }

    /// One tick at `now` (epoch seconds).
    pub fn step(&mut self, now: i64, deps: &mut dyn Deps) -> Step {
        let c = &self.cfg;
        let (poll, quota_poll) = (c.poll, c.quota_poll);
        let (warn_yellow, warn_red, grace) = (c.warn_yellow, c.warn_red, c.grace);

        // The session is the reason the loop exists (0.x `has-session`).
        // `list-sessions` can fail transiently (a client mid-attach, the
        // session-info cache being rewritten), so the session is declared
        // gone only after ALIVE_MISSES consecutive misses.
        if self.alive_checked.is_none_or(|t| now - t >= ALIVE_EVERY) {
            self.alive_checked = Some(now);
            if deps.session_alive() {
                self.alive_misses = 0;
            } else {
                self.alive_misses += 1;
                if self.alive_misses >= ALIVE_MISSES {
                    return Step::Gone;
                }
            }
        }

        // A poke makes a write due on this tick instead of waiting out the
        // poll interval — and because a write is only made from a freshly
        // confirmed end time, a poke re-checks the deadline too. It also
        // means "re-read the quota cache now" (`quota --check` pokes every
        // session after writing a fresh one).
        if deps.take_poke() {
            self.state_written = 0;
            self.quota_checked = 0;
        }

        // Quota, on its own slow cadence; the probe policy (re-probe when the
        // shared cache is stale) lives behind `read_quota`.
        if now - self.quota_checked >= quota_poll {
            self.quota_checked = now;
            self.quota_notice = deps
                .read_quota()
                .filter(|q| q.over)
                .map(|q| notices::quota_notice(q.over_kb, q.hard_kb));
        }

        // Claude Code hint, at the scheduler-poll cadence (one pgrep per
        // poll), not the tick.
        if now - self.hint_checked >= poll {
            self.hint_checked = now;
            self.hint_wanted = deps.claude_hint_wanted();
        }

        if now - self.jobs_checked >= JOBS_EVERY {
            self.jobs_checked = now;
            self.jobs = deps.other_jobs();
        }

        // The notices file, only when the set changes: a steady state must
        // not keep rewriting it.
        let current = self.notices();
        if self.last_notices.as_ref() != Some(&current) {
            deps.write_notices(&current);
            self.last_notices = Some(current);
        }

        // Ask Slurm for the end time immediately before each write, so a
        // `scontrol update TimeLimit` lands within one poll interval and
        // updated_epoch always means "confirmed against Slurm at this time".
        if now - self.state_written >= poll && now - self.end_query >= END_MIN_GAP {
            self.refresh_end_epoch(now, deps);
        }

        // No parseable end time yet (UNLIMITED, squeue hiccup): a plain dot
        // with no countdown, and try again later.
        let Some(end_epoch) = self.end_epoch else {
            self.send(now, deps, Severity::Ok, None);
            return Step::Continue(Duration::from_secs(1));
        };
        let remaining = (end_epoch - now).max(0);

        // Write only on a deadline Slurm just confirmed, so every field in
        // the file was true as of updated_epoch. When squeue is unreachable
        // the file is left alone rather than restamped: it ages honestly and
        // the documented staleness check fires. A failed query leaves
        // state_written unadvanced, so the write stays due and retries.
        if now - self.state_written >= poll && now - self.end_checked <= END_MIN_GAP {
            self.state_written = now;
            deps.write_state(&StateFile {
                job_id: self.cfg.job_id,
                name: self.cfg.name.clone(),
                node: self.cfg.host.clone(),
                end_epoch: Some(end_epoch),
                remaining_seconds: Some(remaining),
                updated_epoch: now,
            });
        }

        if remaining > warn_yellow {
            self.in_red = false;
            self.send(now, deps, Severity::Ok, Some(remaining));
            return Step::Continue(Duration::from_secs(1));
        }
        if remaining > warn_red {
            self.in_red = false;
            self.send(now, deps, Severity::Yellow, Some(remaining));
            return Step::Continue(Duration::from_secs(1));
        }

        // Entering the red phase: confirm the deadline before crying wolf —
        // an extension made moments ago should cancel the alarm, not set it
        // off. Bypasses the rate floor; fires once per entry. A failed query
        // proceeds — fail loud, not silent. (0.x rang the bell here.)
        if !self.in_red {
            if let Some(e) = self.refresh_end_epoch(now, deps) {
                if e - now > warn_red {
                    // Recompute from the new end time next tick.
                    return Step::Continue(Duration::from_millis(200));
                }
            }
            self.in_red = true;
        }

        // Almost out of time: end the session ourselves so teardown runs the
        // normal exit path instead of Slurm's SIGTERM/SIGKILL. end_epoch can
        // be a poll interval old; re-check so an extension inside that
        // window is honoured. A failed query leaves end_epoch alone and the
        // session shuts down: fail safe.
        if remaining <= grace {
            if let Some(e) = self.refresh_end_epoch(now, deps) {
                if e - now > grace {
                    return Step::Continue(Duration::from_millis(200));
                }
            }
            self.last_sent = None;
            self.send(now, deps, Severity::Ending, Some(remaining));
            return Step::Ending;
        }

        self.send(now, deps, Severity::Red, Some(remaining));
        Step::Continue(Duration::from_millis(200))
    }
}

// ---- pure helpers the driver uses ------------------------------------------

/// `2R 1PD` from the user's RUNNING/PENDING jobs other than `self_id`;
/// empty when there are none.
pub fn jobs_summary(rows: &[JobRow], self_id: u64) -> String {
    let (mut running, mut pending) = (0usize, 0usize);
    for r in rows.iter().filter(|r| r.job_id != self_id) {
        match r.state.as_str() {
            "RUNNING" => running += 1,
            "PENDING" => pending += 1,
            _ => {}
        }
    }
    let mut parts = Vec::new();
    if running > 0 {
        parts.push(format!("{running}R"));
    }
    if pending > 0 {
        parts.push(format!("{pending}PD"));
    }
    parts.join(" ")
}

/// Whether the Claude Code integration is installed under `dir`
/// (`$CLAUDE_CONFIG_DIR`, else `~/.claude`): the session-context hook
/// exists and one of the settings files mentions it (script line 1244).
pub fn claude_integration_active(dir: &Path) -> bool {
    if !dir.join("hooks/sinteractive-session-context.sh").exists() {
        return false;
    }
    ["settings.json", "settings.local.json"].iter().any(|f| {
        std::fs::read_to_string(dir.join(f))
            .map(|s| s.contains("sinteractive-session-context"))
            .unwrap_or(false)
    })
}

/// Whether zellij's `list-sessions --no-formatting` output lists `session`
/// as live (an `EXITED` resurrectable entry does not count).
pub fn session_listed(output: &str, session: &str) -> bool {
    output.lines().any(|l| {
        let l = l.trim();
        (l == session || l.starts_with(&format!("{session} "))) && !l.contains("EXITED")
    })
}

/// The environment the zellij server (and so every shell in the session)
/// inherits: the job's, minus every `SLURM_*` variable and any outer zellij
/// identity, plus the session identity and the per-session zellij settings.
pub fn server_env(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
    job_id: u64,
    name: Option<&str>,
    zellij: &ZellijEnv,
) -> HashMap<OsString, OsString> {
    let mut env: HashMap<OsString, OsString> = inherited
        .into_iter()
        .filter(|(k, _)| {
            let k = k.to_string_lossy();
            !(k.starts_with("SLURM_") || k == "ZELLIJ" || k == "ZELLIJ_PANE_ID")
        })
        .collect();
    env.insert("SINTERACTIVE_JOB_ID".into(), job_id.to_string().into());
    match name {
        Some(n) => {
            env.insert("SINTERACTIVE_NAME".into(), n.into());
        }
        None => {
            env.remove(&OsString::from("SINTERACTIVE_NAME"));
        }
    }
    env.insert("TERM".into(), "xterm-256color".into());
    for (k, v) in zellij.env_pairs() {
        env.insert(k.into(), v.into());
    }
    env
}

// ---- the driver --------------------------------------------------------------

static SIGNALLED: AtomicBool = AtomicBool::new(false);
static LAST_SIGNAL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

extern "C" fn on_signal(sig: libc::c_int) {
    LAST_SIGNAL.store(sig, Ordering::SeqCst);
    SIGNALLED.store(true, Ordering::SeqCst);
}

fn last_signal() -> i32 {
    LAST_SIGNAL.load(Ordering::SeqCst)
}

/// Slurm delivers SIGTERM on scancel and at timeout (SIGKILL follows after
/// KillWait): run the teardown on the way out. USR1 is ignored, as in 0.x,
/// so a user's own `--signal=B:USR1` does not kill the batch step.
fn install_signal_handlers() {
    // SAFETY: the handler only stores to an atomic; SIG_IGN is a constant.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGUSR1, libc::SIG_IGN);
    }
}

fn signalled() -> bool {
    SIGNALLED.load(Ordering::SeqCst)
}

/// `SINTERACTIVE_POLL_FAST` as a duration, when set.
fn fast_poll() -> Option<Duration> {
    let v = std::env::var("SINTERACTIVE_POLL_FAST").ok()?;
    v.trim().parse::<f64>().ok().map(Duration::from_secs_f64)
}

/// `d`, capped by `SINTERACTIVE_POLL_FAST`.
fn wait(d: Duration) -> Duration {
    fast_poll().map_or(d, |f| f.min(d))
}

/// Sleep `d` in slices so a signal is noticed promptly.
fn sleep_interruptible(d: Duration) {
    let end = Instant::now() + d;
    while !signalled() {
        let now = Instant::now();
        if now >= end {
            break;
        }
        std::thread::sleep((end - now).min(Duration::from_millis(100)));
    }
}

/// This node's short hostname, as `hostname -s` printed it; falls back to
/// `SLURM_JOB_NODELIST` (a single node for a session job).
fn short_hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most buf.len() bytes into buf.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc == 0 {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let name = String::from_utf8_lossy(&buf[..end]);
        let short = name.split('.').next().unwrap_or("").to_string();
        if !short.is_empty() {
            return short;
        }
    }
    std::env::var("SLURM_JOB_NODELIST").unwrap_or_default()
}

/// The real side effects.
struct NodeDeps<'a> {
    ctx: &'a Ctx,
    zellij: &'a ZellijEnv,
    job_id: u64,
    user: String,
    uid: u32,
    claude_dir: std::path::PathBuf,
}

impl Deps for NodeDeps<'_> {
    fn query_end_epoch(&mut self) -> Option<i64> {
        let row = self.ctx.slurm.job(self.job_id).ok()??;
        slurm_timestamp_to_epoch(&row.end_time)
    }

    fn take_poke(&mut self) -> bool {
        self.ctx.state.take_poke(self.job_id)
    }

    fn read_quota(&mut self) -> Option<QuotaSnapshot> {
        // The probe only runs when the shared cache has gone stale, so N
        // sessions still cost one probe per interval. Every failure is
        // silent: a cluster without the daemons never shows a notice.
        let now = now_epoch();
        let cached = quota::cached(&self.ctx.state);
        let stale = cached
            .as_ref()
            .is_none_or(|q| q.age(now) >= self.ctx.cfg.quota_poll);
        if stale {
            if let Ok(fresh) = quota::probe(&self.ctx.cfg, &self.user, self.uid) {
                let _ = quota::write_cache(&self.ctx.state, &fresh);
                return Some(fresh);
            }
        }
        cached
    }

    fn claude_hint_wanted(&mut self) -> bool {
        // Only while Claude Code is actually running for this user and the
        // integration is not live (script line 1588).
        let running = Command::new("pgrep")
            .args(["-u", &self.uid.to_string(), "-x", "claude"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        running && !claude_integration_active(&self.claude_dir)
    }

    fn other_jobs(&mut self) -> String {
        match self.ctx.slurm.my_jobs(&["RUNNING", "PENDING"]) {
            Ok(rows) => jobs_summary(&rows, self.job_id),
            Err(_) => String::new(),
        }
    }

    fn session_alive(&mut self) -> bool {
        session_alive(self.zellij)
    }

    fn write_state(&mut self, state: &StateFile) {
        if let Err(e) = self.ctx.state.write_state(state) {
            eprintln!("sinteractive: state file: {e}");
        }
    }

    fn write_notices(&mut self, list: &[Notice]) {
        if let Err(e) = notices::write(&self.ctx.state, self.job_id, list) {
            eprintln!("sinteractive: notices file: {e}");
        }
    }

    /// Pipe the status message to the plugin, never waiting on it: a CLI
    /// pipe blocks until the plugin handles it, and zellij loads a layout's
    /// plugins only when a client first renders them — before anyone attaches
    /// the pipe would block the loop forever. Kill it after two seconds.
    fn send_status(&mut self, msg: &StatusMsg) {
        let Ok(json) = serde_json::to_string(msg) else {
            return;
        };
        let mut child = match self
            .zellij
            .command(["pipe", "--name", PIPE_NAME, "--", &json])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
    }
}

/// Whether `list-sessions` shows this session live.
fn session_alive(zellij: &ZellijEnv) -> bool {
    let Ok(out) = zellij
        .command(["list-sessions", "--no-formatting"])
        .stdin(Stdio::null())
        .output()
    else {
        return false;
    };
    out.status.success() && session_listed(&String::from_utf8_lossy(&out.stdout), &zellij.session())
}

/// Start the headless server and wait until it lists the session.
fn start_server(
    zellij: &ZellijEnv,
    config: &Path,
    env: &HashMap<OsString, OsString>,
) -> Result<Child> {
    let session = zellij.session();
    let mut cmd = zellij.command([
        "--config",
        &config.to_string_lossy(),
        "attach",
        "--create-background",
        &session,
    ]);
    cmd.env_clear();
    cmd.envs(env);
    // zellij refuses to `attach` when ZELLIJ_SESSION_NAME names the target
    // ("attach to the current session"); the server sets it for its panes.
    cmd.env_remove("ZELLIJ_SESSION_NAME");
    cmd.stdin(Stdio::null());
    let mut child = cmd.spawn().context("start the zellij server")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    // Do not touch the socket until the create-background client has
    // finished: it spawns the server and then configures the session, and a
    // probe client (list-sessions connects and disconnects) that arrives in
    // between panics zellij-server (RemoveClient with no session data yet,
    // zellij-server/src/lib.rs:1462 in 0.45.1). Seen on a real job.
    let mut client_done = false;
    loop {
        if !client_done {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => client_done = true,
                Ok(Some(status)) => {
                    bail!("zellij server exited ({status}) before session {session} came up")
                }
                _ => {}
            }
        } else if session_alive(zellij) {
            return Ok(child);
        }
        if signalled() {
            bail!("interrupted while starting the session");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            bail!("zellij session {session} did not come up within 30s");
        }
        std::thread::sleep(wait(Duration::from_millis(200)));
    }
}

/// Type the farewell into the shell, pause, and end the session.
fn end_session(zellij: &ZellijEnv) {
    let line = format!("\r{ENDING_LINE}\r");
    let _ = zellij
        .command(["action", "write-chars", "-p", "terminal_0", &line])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(wait(Duration::from_secs(2)));
    kill_session(zellij);
}

fn kill_session(zellij: &ZellijEnv) {
    let _ = zellij
        .command(["kill-session", &zellij.session()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn run(args: JobArgs) -> Result<i32> {
    let job_id: u64 = std::env::var("SLURM_JOB_ID")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(|| anyhow!("__job must run under sbatch (SLURM_JOB_ID is not set)"))?;
    let ctx = Ctx::new();
    let cfg = &ctx.cfg;
    let name = args.session_name.clone().filter(|n| !n.is_empty());
    let host = short_hostname();

    let bundle = bundle::ensure(cfg, args.mouse)?;
    let zellij = ZellijEnv::new(cfg, job_id)?;
    std::fs::create_dir_all(&zellij.socket_dir)
        .with_context(|| format!("create {}", zellij.socket_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&zellij.socket_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    zellij_cmd::grant_plugin_permissions(&zellij.xdg_cache_home, &bundle.plugin)
        .context("pre-grant the status plugin's permissions")?;

    install_signal_handlers();

    let env = server_env(std::env::vars_os(), job_id, name.as_deref(), &zellij);
    let mut server = start_server(&zellij, &bundle.config, &env)?;
    // The config the client side must attach with (mouse mode is a client
    // setting), then the marker the launcher polls for.
    let _ = std::fs::write(
        zellij_cmd::config_marker(job_id),
        bundle.config.to_string_lossy().as_bytes(),
    );
    std::fs::write(zellij_cmd::ready_marker(job_id), now_epoch().to_string())
        .context("write the ready marker")?;

    let mut lcfg = LoopConfig::from_config(cfg, job_id, name, host);
    lcfg.maint = args.maint.as_deref().and_then(parse_maint);
    let (user, uid) = quota::current_user();
    let claude_dir = match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(d) if !d.is_empty() => std::path::PathBuf::from(d),
        _ => std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".claude"),
    };
    let mut deps = NodeDeps {
        ctx: &ctx,
        zellij: &zellij,
        job_id,
        user,
        uid,
        claude_dir,
    };
    let mut lp = JobLoop::new(lcfg);
    let mut ended_by_us = false;
    while !signalled() {
        match lp.step(now_epoch(), &mut deps) {
            Step::Continue(d) => sleep_interruptible(wait(d)),
            Step::Ending => {
                end_session(&zellij);
                ended_by_us = true;
                break;
            }
            Step::Gone => {
                eprintln!(
                    "sinteractive: session {} is no longer listed by zellij; ending the job",
                    zellij.session()
                );
                break;
            }
        }
    }
    if signalled() && !ended_by_us {
        // scancel / timeout: take the session down with us.
        eprintln!(
            "sinteractive: signal {} received; ending the session",
            last_signal()
        );
        kill_session(&zellij);
    }

    // Teardown: the state files (they must not outlive the job), the
    // node-local socket dir, and the server's client process if it is
    // still around.
    ctx.state.cleanup(job_id);
    let _ = std::fs::remove_dir_all(&zellij.socket_dir);
    if let Ok(None) = server.try_wait() {
        let _ = server.kill();
        let _ = server.wait();
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        end: Option<i64>,
        queries: usize,
        poke: bool,
        quota: Option<QuotaSnapshot>,
        hint: bool,
        jobs: String,
        alive: bool,
        states: Vec<StateFile>,
        notices: Vec<Vec<Notice>>,
        sent: Vec<StatusMsg>,
    }

    impl Fake {
        fn alive(end: Option<i64>) -> Self {
            Fake {
                end,
                alive: true,
                ..Default::default()
            }
        }
    }

    impl Deps for Fake {
        fn query_end_epoch(&mut self) -> Option<i64> {
            self.queries += 1;
            self.end
        }
        fn take_poke(&mut self) -> bool {
            std::mem::take(&mut self.poke)
        }
        fn read_quota(&mut self) -> Option<QuotaSnapshot> {
            self.quota.clone()
        }
        fn claude_hint_wanted(&mut self) -> bool {
            self.hint
        }
        fn other_jobs(&mut self) -> String {
            self.jobs.clone()
        }
        fn session_alive(&mut self) -> bool {
            self.alive
        }
        fn write_state(&mut self, s: &StateFile) {
            self.states.push(s.clone());
        }
        fn write_notices(&mut self, n: &[Notice]) {
            self.notices.push(n.to_vec());
        }
        fn send_status(&mut self, m: &StatusMsg) {
            self.sent.push(m.clone());
        }
    }

    const T0: i64 = 1_800_000_000;

    fn cfg() -> LoopConfig {
        LoopConfig {
            job_id: 4242,
            name: Some("t".into()),
            host: "node01".into(),
            warn_yellow: 3600,
            warn_red: 600,
            grace: 10,
            poll: 30,
            quota_poll: 600,
            maint: None,
        }
    }

    #[test]
    fn state_written_only_after_a_confirmed_deadline() {
        let mut f = Fake::alive(None);
        let mut lp = JobLoop::new(cfg());
        assert_eq!(lp.step(T0, &mut f), Step::Continue(Duration::from_secs(1)));
        assert_eq!(f.queries, 1);
        assert!(f.states.is_empty(), "no deadline, no file");
        let m = f.sent.last().unwrap();
        assert_eq!(m.severity, Severity::Ok);
        assert_eq!(m.remaining, "");
        assert_eq!(m.remaining_secs, None);

        // Slurm answers on a later tick (past the 5 s floor): written once.
        f.end = Some(T0 + 8 * 3600);
        lp.step(T0 + 5, &mut f);
        assert_eq!(f.states.len(), 1);
        let s = &f.states[0];
        assert_eq!(s.job_id, 4242);
        assert_eq!(s.name.as_deref(), Some("t"));
        assert_eq!(s.node, "node01");
        assert_eq!(s.end_epoch, Some(T0 + 8 * 3600));
        assert_eq!(s.remaining_seconds, Some(8 * 3600 - 5));
        assert_eq!(s.updated_epoch, T0 + 5);
        assert_eq!(f.sent.last().unwrap().remaining, "7h 59m");

        // Not rewritten until the poll interval passes …
        lp.step(T0 + 20, &mut f);
        assert_eq!(f.states.len(), 1);
        // … then it is, with a fresh query first.
        let q = f.queries;
        lp.step(T0 + 35, &mut f);
        assert_eq!(f.queries, q + 1);
        assert_eq!(f.states.len(), 2);
        assert_eq!(f.states[1].updated_epoch, T0 + 35);
    }

    #[test]
    fn failed_query_never_restamps_but_the_countdown_continues() {
        let mut f = Fake::alive(Some(T0 + 7200));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        assert_eq!(f.states.len(), 1);
        // Scheduler goes away: the write stays due and retries, the file is
        // left alone, the status keeps counting from the last deadline.
        f.end = None;
        for t in [30, 36, 42, 60] {
            lp.step(T0 + t, &mut f);
        }
        assert_eq!(f.states.len(), 1, "never restamped on a failed query");
        assert!(f.queries >= 4, "retried at the 5 s floor: {}", f.queries);
        let m = f.sent.last().unwrap();
        assert_eq!(m.remaining_secs, Some(7200 - 60));
        // Back: written at once (the write was pending), with the new deadline.
        f.end = Some(T0 + 9000);
        lp.step(T0 + 66, &mut f);
        assert_eq!(f.states.len(), 2);
        assert_eq!(f.states[1].end_epoch, Some(T0 + 9000));
        assert_eq!(f.states[1].updated_epoch, T0 + 66);
    }

    #[test]
    fn rate_floor_holds_between_queries() {
        let mut f = Fake::alive(None);
        let mut lp = JobLoop::new(cfg());
        for t in 0..5 {
            lp.step(T0 + t, &mut f);
        }
        assert_eq!(f.queries, 1, "one query inside the 5 s floor");
        lp.step(T0 + 5, &mut f);
        assert_eq!(f.queries, 2);
    }

    #[test]
    fn poke_forces_a_requery_and_a_write() {
        let mut f = Fake::alive(Some(T0 + 7200));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        assert_eq!(f.states.len(), 1);
        // Inside the poll interval nothing happens …
        lp.step(T0 + 10, &mut f);
        assert_eq!(f.states.len(), 1);
        // … until a poke: re-query (the deadline moved) and write.
        f.poke = true;
        f.end = Some(T0 + 10_000);
        let q = f.queries;
        lp.step(T0 + 12, &mut f);
        assert_eq!(f.queries, q + 1);
        assert_eq!(f.states.len(), 2);
        assert_eq!(f.states[1].end_epoch, Some(T0 + 10_000));
        assert_eq!(f.states[1].updated_epoch, T0 + 12);
    }

    #[test]
    fn poke_inside_the_query_floor_writes_from_the_still_fresh_deadline() {
        let mut f = Fake::alive(Some(T0 + 7200));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        f.poke = true;
        let q = f.queries;
        lp.step(T0 + 2, &mut f);
        assert_eq!(f.queries, q, "the floor holds");
        assert_eq!(f.states.len(), 2, "confirmed 2 s ago is fresh enough");
        assert_eq!(f.states[1].updated_epoch, T0 + 2);
        assert_eq!(f.states[1].remaining_seconds, Some(7198));
    }

    #[test]
    fn severity_phases_at_the_thresholds() {
        let end = T0 + 10_000;
        let mut f = Fake::alive(Some(end));
        let mut lp = JobLoop::new(cfg());
        let sev = |lp: &mut JobLoop, f: &mut Fake, now: i64| {
            let step = lp.step(now, f);
            (f.sent.last().unwrap().clone(), step)
        };
        let (m, step) = sev(&mut lp, &mut f, end - 3601);
        assert_eq!(m.severity, Severity::Ok);
        assert_eq!(m.remaining, "1h");
        assert_eq!(step, Step::Continue(Duration::from_secs(1)));

        let (m, step) = sev(&mut lp, &mut f, end - 3600);
        assert_eq!(m.severity, Severity::Yellow, "≤ warn_yellow");
        assert_eq!(m.remaining, "1h");
        assert_eq!(step, Step::Continue(Duration::from_secs(1)));

        let (m, _) = sev(&mut lp, &mut f, end - 601);
        assert_eq!(m.severity, Severity::Yellow);
        assert_eq!(m.remaining, "10m");

        // Entering red re-confirms the deadline once, then spins at 0.2 s.
        let q = f.queries;
        let (m, step) = sev(&mut lp, &mut f, end - 600);
        assert_eq!(f.queries, q + 1, "deadline re-confirmed on entry");
        assert_eq!(m.severity, Severity::Red, "≤ warn_red");
        assert_eq!(m.remaining, "10:00");
        assert_eq!(m.remaining_secs, Some(600));
        assert_eq!(step, Step::Continue(Duration::from_millis(200)));
        let (m, _) = sev(&mut lp, &mut f, end - 65);
        assert_eq!(m.severity, Severity::Red);
        assert_eq!(m.remaining, "1:05");
        assert_eq!(
            f.states.last().unwrap().updated_epoch,
            end - 65,
            "the poll-cadence write goes on while red"
        );

        // Ending at grace.
        let (m, step) = sev(&mut lp, &mut f, end - 10);
        assert_eq!(step, Step::Ending);
        assert_eq!(m.severity, Severity::Ending);
        assert_eq!(m.remaining, "0:10");
    }

    #[test]
    fn extension_on_entering_red_cancels_the_alarm() {
        let end = T0 + 620;
        let mut f = Fake::alive(Some(end));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        assert_eq!(f.sent.last().unwrap().severity, Severity::Yellow);
        // Slurm extended the job inside the poll window, just before the
        // red boundary: the entry re-check sees it and no spinner starts.
        f.end = Some(end + 300);
        let q = f.queries;
        let step = lp.step(T0 + 25, &mut f);
        assert_eq!(f.queries, q + 1);
        assert_eq!(step, Step::Continue(Duration::from_millis(200)));
        assert!(!lp.in_red);
        lp.step(T0 + 26, &mut f);
        let m = f.sent.last().unwrap();
        assert_eq!(m.severity, Severity::Yellow);
        assert_eq!(m.remaining_secs, Some(end + 300 - T0 - 26));
    }

    #[test]
    fn ending_rechecks_and_honours_an_extension() {
        let end = T0 + 1000;
        let mut f = Fake::alive(Some(end));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        lp.step(end - 25, &mut f); // poll-cadence query, then red entry
        assert!(lp.in_red);
        assert_eq!(f.states.last().unwrap().updated_epoch, end - 25);
        // Extended inside the poll window: only the grace re-check can see it.
        f.end = Some(end + 600);
        let step = lp.step(end - 8, &mut f);
        assert_eq!(step, Step::Continue(Duration::from_millis(200)));
        // Next tick recomputes from the new deadline: yellow again.
        lp.step(end - 7, &mut f);
        assert_eq!(f.sent.last().unwrap().severity, Severity::Yellow);
        assert!(!lp.in_red);

        // A failed re-check at the end shuts down: fail safe.
        f.end = None;
        let step = lp.step(end + 600 - 3, &mut f);
        assert_eq!(step, Step::Ending);
        assert_eq!(f.sent.last().unwrap().severity, Severity::Ending);
    }

    #[test]
    fn gone_when_the_session_disappears() {
        let mut f = Fake::alive(Some(T0 + 7200));
        let mut lp = JobLoop::new(cfg());
        assert!(matches!(lp.step(T0, &mut f), Step::Continue(_)));
        f.alive = false;
        // Checked every ALIVE_EVERY seconds, not every tick, and a single
        // miss is not enough: list-sessions can fail transiently.
        assert!(matches!(lp.step(T0 + 1, &mut f), Step::Continue(_)));
        assert!(matches!(lp.step(T0 + 2, &mut f), Step::Continue(_)));
        assert!(matches!(lp.step(T0 + 4, &mut f), Step::Continue(_)));
        assert_eq!(lp.step(T0 + 6, &mut f), Step::Gone);
        // A hit in between resets the count.
        let mut f = Fake::alive(Some(T0 + 7200));
        let mut lp = JobLoop::new(cfg());
        assert!(matches!(lp.step(T0, &mut f), Step::Continue(_)));
        f.alive = false;
        assert!(matches!(lp.step(T0 + 2, &mut f), Step::Continue(_)));
        assert!(matches!(lp.step(T0 + 4, &mut f), Step::Continue(_)));
        f.alive = true;
        assert!(matches!(lp.step(T0 + 6, &mut f), Step::Continue(_)));
        f.alive = false;
        assert!(matches!(lp.step(T0 + 8, &mut f), Step::Continue(_)));
        assert!(matches!(lp.step(T0 + 10, &mut f), Step::Continue(_)));
        assert_eq!(lp.step(T0 + 12, &mut f), Step::Gone);
    }

    #[test]
    fn notices_are_assembled_in_order_and_written_on_change() {
        let mut c = cfg();
        c.maint = parse_maint("maint-2026-09@1788422100");
        let mut f = Fake::alive(Some(T0 + 7200));
        f.quota = Some(quota::snapshot("tester", 2048, 1024, T0));
        f.hint = true;
        f.jobs = "2R 1PD".into();
        let mut lp = JobLoop::new(c);
        lp.step(T0, &mut f);
        assert_eq!(f.notices.len(), 1);
        let kinds: Vec<&str> = f.notices[0].iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["quota", "maint", "hint"]);
        assert_eq!(f.notices[0][0].text, "QUOTA over by 1M (1M limit)");
        assert!(f.notices[0][1].text.contains("(maint-2026-09)"));
        let m = f.sent.last().unwrap();
        assert_eq!(m.notices.len(), 3);
        assert_eq!(m.jobs, "2R 1PD");
        assert_eq!(m.help.len(), 2);
        assert_eq!(m.help[0][0], ("n".to_string(), "notices".to_string()));
        assert_eq!(m.host, "node01");
        assert_eq!(m.job_id, 4242);

        // Steady state: not rewritten.
        lp.step(T0 + 1, &mut f);
        assert_eq!(f.notices.len(), 1);

        // Quota clears after a poke (re-read now, not at the 10 min poll);
        // claude goes away at the next hint poll.
        f.quota = Some(quota::snapshot("tester", 512, 1024, T0));
        f.hint = false;
        f.poke = true;
        lp.step(T0 + 6, &mut f);
        assert_eq!(f.notices.len(), 2);
        let kinds: Vec<&str> = f.notices[1].iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["maint", "hint"], "hint waits for the poll");
        lp.step(T0 + 30, &mut f);
        let kinds: Vec<&str> = f.notices[2].iter().map(|n| n.kind.as_str()).collect();
        assert_eq!(kinds, ["maint"]);
    }

    #[test]
    fn status_is_piped_once_per_second() {
        let mut f = Fake::alive(Some(T0 + 7200));
        let mut lp = JobLoop::new(cfg());
        for _ in 0..5 {
            lp.step(T0, &mut f);
        }
        assert_eq!(f.sent.len(), 1);
        lp.step(T0 + 1, &mut f);
        assert_eq!(f.sent.len(), 2);
        assert_eq!(f.sent[1].sent_epoch, T0 + 1);
        assert_eq!(f.sent[1].name.as_deref(), Some("t"));
    }

    #[test]
    fn maint_spec_parsing() {
        assert_eq!(
            parse_maint("maint-2026-09@1788422100"),
            Some(("maint-2026-09".into(), 1788422100))
        );
        assert_eq!(parse_maint("a@b@5"), Some(("a@b".into(), 5)));
        assert_eq!(parse_maint("@5"), None);
        assert_eq!(parse_maint("x@"), None);
        assert_eq!(parse_maint("x"), None);
    }

    #[test]
    fn jobs_summary_forms() {
        let row = |id: u64, state: &str| JobRow {
            job_id: id,
            state: state.into(),
            ..JobRow::default()
        };
        let rows = vec![
            row(1, "RUNNING"),
            row(2, "RUNNING"),
            row(3, "PENDING"),
            row(4, "COMPLETING"),
        ];
        assert_eq!(jobs_summary(&rows, 99), "2R 1PD");
        assert_eq!(jobs_summary(&rows, 1), "1R 1PD");
        assert_eq!(jobs_summary(&rows[2..], 99), "1PD");
        assert_eq!(jobs_summary(&rows[..2], 99), "2R");
        assert_eq!(jobs_summary(&[], 99), "");
    }

    #[test]
    fn claude_integration_detection() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        assert!(!claude_integration_active(d));
        std::fs::create_dir_all(d.join("hooks")).unwrap();
        std::fs::write(
            d.join("hooks/sinteractive-session-context.sh"),
            "#!/bin/sh\n",
        )
        .unwrap();
        assert!(!claude_integration_active(d), "hook alone is not enough");
        std::fs::write(d.join("settings.json"), "{\"hooks\":{}}").unwrap();
        assert!(!claude_integration_active(d));
        std::fs::write(
            d.join("settings.local.json"),
            "{\"hooks\":{\"SessionStart\":[\"sinteractive-session-context.sh\"]}}",
        )
        .unwrap();
        assert!(claude_integration_active(d));
    }

    #[test]
    fn session_listing() {
        let out = "sinteractive-4242 [Created 1s ago]\nother [Created 2m ago]\n";
        assert!(session_listed(out, "sinteractive-4242"));
        assert!(!session_listed(out, "sinteractive-424"));
        assert!(!session_listed(out, "sinteractive-42421"));
        assert!(!session_listed(
            "No active zellij sessions found.\n",
            "sinteractive-4242"
        ));
        assert!(!session_listed(
            "sinteractive-4242 [EXITED]\n",
            "sinteractive-4242"
        ));
        assert!(session_listed("sinteractive-4242\n", "sinteractive-4242"));
    }

    #[test]
    fn server_env_strips_slurm_and_adds_identity() {
        let mut cfg = Config::defaults();
        cfg.cache_dir = "/c".into();
        let z = ZellijEnv {
            job_id: 7,
            socket_dir: "/tmp/sint-7".into(),
            xdg_cache_home: "/c/xdg".into(),
            exe: "/x/sinteractive".into(),
        };
        let inherited: Vec<(OsString, OsString)> = [
            ("SLURM_JOB_ID", "7"),
            ("SLURM_NTASKS", "1"),
            ("ZELLIJ", "0"),
            ("ZELLIJ_PANE_ID", "3"),
            ("HOME", "/h"),
            ("TERM", "screen"),
            ("SINTERACTIVE_NAME", "stale"),
        ]
        .iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect();
        let env = server_env(inherited.clone(), 7, Some("web"), &z);
        let get = |k: &str| {
            env.get(&OsString::from(k))
                .map(|v| v.to_string_lossy().into_owned())
        };
        assert!(!env
            .keys()
            .any(|k| k.to_string_lossy().starts_with("SLURM_")));
        assert_eq!(get("ZELLIJ"), None);
        assert_eq!(get("ZELLIJ_PANE_ID"), None);
        assert_eq!(get("HOME").as_deref(), Some("/h"));
        assert_eq!(get("TERM").as_deref(), Some("xterm-256color"));
        assert_eq!(get("SINTERACTIVE_JOB_ID").as_deref(), Some("7"));
        assert_eq!(get("SINTERACTIVE_NAME").as_deref(), Some("web"));
        assert_eq!(get("ZELLIJ_SOCKET_DIR").as_deref(), Some("/tmp/sint-7"));
        assert_eq!(get("XDG_CACHE_HOME").as_deref(), Some("/c/xdg"));
        assert_eq!(
            get("ZELLIJ_SESSION_NAME").as_deref(),
            Some("sinteractive-7")
        );
        let env = server_env(inherited, 7, None, &z);
        assert!(!env.contains_key(&OsString::from("SINTERACTIVE_NAME")));
    }
}
