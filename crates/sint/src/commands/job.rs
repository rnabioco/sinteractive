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
//! Host monitoring rides on the same loop: a [`Sampler`] scoped to this
//! job is read every [`SAMPLE_EVERY`] seconds and written to
//! `<jobid>.metrics.json` every [`METRICS_EVERY`] (what `monitor` reads on
//! a login node); it also feeds the bar's `cpu 34% 12/32G` / `gpu0 87%`
//! fields and the monitor panel's own-host entry. The user's other RUNNING
//! jobs are sampled every [`REMOTE_EVERY`] seconds on a background thread
//! (`ssh NODE sinteractive snapshot --json --job ID`, or that job's own
//! fresh `<id>.metrics.json` when it is an sinteractive session) and appear
//! as further panel entries.
//!
//! Session events (`started`, `walltime_warn`, `walltime_red`,
//! `quota_over`, `job_done`, `gpu_idle`, `ended`) are appended to
//! `<jobid>.events.ndjson` through [`Deps::emit_event`]; see
//! [`sint_core::events`] for the shapes.
//!
//! Undocumented knob for the tests: `SINTERACTIVE_POLL_FAST=<secs>` caps
//! every wait in this file (server readiness poll, loop tick, pre-kill
//! pause).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use sint_core::config::Config;
use sint_core::events::{self, Event};
use sint_core::metrics::{self, Sampler, Snapshot};
use sint_core::notices::{self, Notice};
use sint_core::now_epoch;
use sint_core::quota::{self, QuotaSnapshot};
use sint_core::slurm::squeue::JobRow;
use sint_core::slurm::Slurm;
use sint_core::state::{StateDir, StateFile};
use sint_core::time::{format_short_duration, slurm_timestamp_to_epoch};
use sint_proto::{HostPanel, Severity, StatusMsg, PIPE_NAME};

use super::common::{ssh_batch, Ctx};
use crate::bundle;
use crate::cli::JobArgs;
use crate::zellij_cmd::{self, shell_quote, ZellijEnv};

/// Floor on scheduler queries however often the loop wakes, and the window
/// in which a confirmed end time still counts as fresh enough to write.
pub const END_MIN_GAP: i64 = 5;
/// How often the `2R 1PD` other-jobs summary is refreshed.
pub const JOBS_EVERY: i64 = 30;
/// How often `list-sessions` is consulted to see whether the session is up.
pub const ALIVE_EVERY: i64 = 2;
/// Consecutive alive-check misses before the session counts as gone.
const ALIVE_MISSES: u32 = 3;
/// How often the local host sampler is read.
pub const SAMPLE_EVERY: i64 = 2;
/// How often the local snapshot is written to `<jobid>.metrics.json`.
pub const METRICS_EVERY: i64 = 5;
/// How often the user's other running jobs are sampled on their nodes.
pub const REMOTE_EVERY: i64 = 10;
/// How long one round of remote `snapshot` calls may take before the
/// stragglers are killed.
pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(15);
/// A remote job's own `<id>.metrics.json` no older than this stands in
/// for an ssh round trip.
pub const REMOTE_CACHE_FRESH: i64 = REMOTE_EVERY;
/// `walltime_warn` fires when this much (or less) is left.
pub const WALLTIME_WARN_SECS: i64 = 1800;
/// `walltime_red` fires when this much (or less) is left.
pub const WALLTIME_RED_SECS: i64 = 600;
/// A held GPU under this utilisation …
pub const GPU_IDLE_UTIL: u8 = 5;
/// … for this long is `gpu_idle`.
pub const GPU_IDLE_AFTER: i64 = 600;
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
    /// The session name Slurm currently carries in the job's Comment
    /// (`Some(name)` once known; `None` before the first query). A rename
    /// (`Ctrl+b $`) rewrites the Comment, and the loop follows it.
    fn current_name(&mut self) -> Option<Option<String>> {
        None
    }
    /// Consume a pending `<jobid>.poke` (true when one was there).
    fn take_poke(&mut self) -> bool;
    /// The current quota snapshot (cache, re-probed when stale); `None`
    /// when the cluster has no quota daemons or nothing answered.
    fn read_quota(&mut self) -> Option<QuotaSnapshot>;
    /// Whether the Claude Code install hint is due (a `claude` process is
    /// running for this user and the integration is not installed).
    fn claude_hint_wanted(&mut self) -> bool;
    /// The user's RUNNING/PENDING jobs (this one included); `None` when
    /// squeue failed, so a hiccup is never read as "every job finished".
    fn other_jobs(&mut self) -> Option<Vec<QueueJob>>;
    /// Whether zellij still lists the session.
    fn session_alive(&mut self) -> bool;
    /// Write `<jobid>.json`.
    fn write_state(&mut self, state: &StateFile);
    /// Write (or remove, when empty) `<jobid>.notices`.
    fn write_notices(&mut self, notices: &[Notice]);
    /// Pipe a message to the status plugin.
    fn send_status(&mut self, msg: &StatusMsg);
    /// One sample of this host, scoped to the job; `None` when sampling
    /// is unavailable.
    fn sample_local(&mut self) -> Option<Snapshot>;
    /// Write `<job_id>.metrics.json`.
    fn write_metrics(&mut self, job_id: u64, snap: &Snapshot);
    /// Remove `<job_id>.metrics.json` (a remote job that left the queue).
    fn remove_metrics(&mut self, job_id: u64);
    /// Start sampling these jobs on their nodes in the background; results
    /// come back through [`Deps::take_remote`]. Never blocks.
    fn poll_remote(&mut self, targets: &[RemoteTarget]);
    /// Remote samples that have finished since the last call.
    fn take_remote(&mut self) -> Vec<RemoteSnapshot>;
    /// Append to `<jobid>.events.ndjson`.
    fn emit_event(&mut self, event: &Event);
}

/// One of the user's queued jobs, as the loop sees it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueJob {
    pub job_id: u64,
    /// `RUNNING` / `PENDING`.
    pub state: String,
    /// Raw `%N` nodelist (`c3cpu-a2-u[3-4]`), empty while pending.
    pub node: String,
    /// squeue's `%j` job name.
    pub name: Option<String>,
}

/// A job to sample on its node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTarget {
    pub job_id: u64,
    pub node: String,
}

/// A finished remote sample.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteSnapshot {
    pub job_id: u64,
    pub snapshot: Snapshot,
    /// Taken over ssh (so worth writing to `<id>.metrics.json`), as opposed
    /// to read back from that job's own fresh file.
    pub fetched: bool,
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
    /// The user's queue as of the last successful refresh.
    queue: Vec<QueueJob>,
    /// Other jobs seen RUNNING/PENDING, by id, for `job_done`.
    seen_jobs: HashMap<u64, Option<String>>,
    alive_checked: Option<i64>,
    /// Consecutive failed alive checks.
    alive_misses: u32,
    /// Second in which the last status message went out.
    last_sent: Option<i64>,
    /// Whether the red phase has been entered (0.x `belled`): the deadline
    /// is re-confirmed once on entry before the spinner starts.
    in_red: bool,
    /// Latest local sample and when it was taken / last written out.
    local: Option<Snapshot>,
    sampled: i64,
    metrics_written: i64,
    remote_polled: i64,
    /// Latest sample per other running job, in job-id order.
    remotes: BTreeMap<u64, Snapshot>,
    /// Event bookkeeping: `started` sent; warn/red armed while above the
    /// threshold; quota over as of the last check; per-GPU idle episodes.
    started: bool,
    warned: bool,
    red_warned: bool,
    quota_over: bool,
    gpu_idle_since: HashMap<u32, i64>,
    gpu_idle_sent: HashSet<u32>,
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
            queue: Vec::new(),
            seen_jobs: HashMap::new(),
            alive_checked: None,
            alive_misses: 0,
            last_sent: None,
            in_red: false,
            local: None,
            sampled: 0,
            metrics_written: 0,
            remote_polled: 0,
            remotes: BTreeMap::new(),
            started: false,
            warned: false,
            red_warned: false,
            quota_over: false,
            gpu_idle_since: HashMap::new(),
            gpu_idle_sent: HashSet::new(),
        }
    }

    /// Ask Slurm now, bypassing the rate floor. Returns the confirmed end
    /// time; a failure leaves the previous one alone.
    fn refresh_end_epoch(&mut self, now: i64, deps: &mut dyn Deps) -> Option<i64> {
        self.end_query = now;
        let e = deps.query_end_epoch();
        // The same query read the Comment; a rename shows up here.
        if let Some(name) = deps.current_name() {
            self.cfg.name = name;
        }
        let e = e?;
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

    /// The monitor panel's hosts: this node first, then every other
    /// running job that has answered, by job id.
    fn hosts(&self, now: i64) -> Vec<HostPanel> {
        let mut v = Vec::new();
        if let Some(s) = &self.local {
            v.push(s.to_host_panel(self.cfg.job_id, self.cfg.name.clone(), s.age_secs(now)));
        }
        for (id, s) in &self.remotes {
            let name = self
                .queue
                .iter()
                .find(|j| j.job_id == *id)
                .and_then(|j| j.name.clone());
            v.push(s.to_host_panel(*id, name, s.age_secs(now)));
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
            load: self.local.as_ref().map(load_line).unwrap_or_default(),
            gpu: self.local.as_ref().map(gpu_line).unwrap_or_default(),
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
            hosts: self.hosts(now),
            sent_epoch: now,
        }
    }

    // ---- monitoring and events -------------------------------------------

    /// The `started` event, on the first tick only.
    fn emit_started(&mut self, now: i64, deps: &mut dyn Deps) {
        if self.started {
            return;
        }
        self.started = true;
        let ev = Event::at(now, "started")
            .with("job", self.cfg.job_id)
            .with("node", self.cfg.host.as_str())
            .with("name", self.cfg.name.clone());
        deps.emit_event(&ev);
    }

    /// Read the local sampler on its own cadence, write the snapshot out
    /// on a slower one, and watch the GPUs for idleness.
    fn sample_local(&mut self, now: i64, deps: &mut dyn Deps) {
        if now - self.sampled >= SAMPLE_EVERY {
            self.sampled = now;
            if let Some(snap) = deps.sample_local() {
                self.check_gpu_idle(now, &snap, deps);
                self.local = Some(snap);
            }
        }
        if now - self.metrics_written >= METRICS_EVERY {
            if let Some(snap) = &self.local {
                self.metrics_written = now;
                deps.write_metrics(self.cfg.job_id, snap);
            }
        }
    }

    /// `gpu_idle`: a GPU in the scope under [`GPU_IDLE_UTIL`] for
    /// [`GPU_IDLE_AFTER`] while a process holds it, once per episode.
    fn check_gpu_idle(&mut self, now: i64, snap: &Snapshot, deps: &mut dyn Deps) {
        let mut idle_now = HashSet::new();
        for g in &snap.gpus {
            if g.util_pct >= GPU_IDLE_UTIL || g.procs.is_empty() {
                continue;
            }
            idle_now.insert(g.index);
            let since = *self.gpu_idle_since.entry(g.index).or_insert(now);
            if now - since >= GPU_IDLE_AFTER && self.gpu_idle_sent.insert(g.index) {
                let ev = Event::at(now, "gpu_idle")
                    .with("gpu", g.index)
                    .with("util_pct", g.util_pct)
                    .with("idle_secs", now - since);
                deps.emit_event(&ev);
            }
        }
        self.gpu_idle_since.retain(|i, _| idle_now.contains(i));
        self.gpu_idle_sent.retain(|i| idle_now.contains(i));
    }

    /// Refresh the queue: the `2R 1PD` summary, `job_done` for any other
    /// job that left, and the remote sample set (dropping hosts whose job
    /// is gone).
    fn refresh_queue(&mut self, now: i64, deps: &mut dyn Deps) {
        let Some(queue) = deps.other_jobs() else {
            return;
        };
        let self_id = self.cfg.job_id;
        let present: HashMap<u64, Option<String>> = queue
            .iter()
            .filter(|j| j.job_id != self_id)
            .map(|j| (j.job_id, j.name.clone()))
            .collect();
        let mut gone: Vec<(u64, Option<String>)> = self
            .seen_jobs
            .iter()
            .filter(|(id, _)| !present.contains_key(id))
            .map(|(id, name)| (*id, name.clone()))
            .collect();
        gone.sort_unstable_by_key(|(id, _)| *id);
        for (id, name) in gone {
            let ev = Event::at(now, "job_done")
                .with("job", id)
                .with("name", name);
            deps.emit_event(&ev);
        }
        self.seen_jobs = present;

        let running: HashSet<u64> = queue
            .iter()
            .filter(|j| j.state == "RUNNING" && j.job_id != self_id)
            .map(|j| j.job_id)
            .collect();
        let dropped: Vec<u64> = self
            .remotes
            .keys()
            .filter(|id| !running.contains(id))
            .copied()
            .collect();
        for id in dropped {
            self.remotes.remove(&id);
            deps.remove_metrics(id);
        }
        self.jobs = jobs_summary(&queue, self_id);
        self.queue = queue;
    }

    /// The other running jobs not on this node.
    fn remote_targets(&self) -> Vec<RemoteTarget> {
        self.queue
            .iter()
            .filter(|j| j.state == "RUNNING" && j.job_id != self.cfg.job_id)
            .filter_map(|j| {
                let node = first_node(&j.node)?;
                (node != self.cfg.host).then_some(RemoteTarget {
                    job_id: j.job_id,
                    node,
                })
            })
            .collect()
    }

    /// Kick off a remote round on its cadence and absorb whatever has
    /// come back.
    fn poll_remotes(&mut self, now: i64, deps: &mut dyn Deps) {
        if now - self.remote_polled >= REMOTE_EVERY {
            self.remote_polled = now;
            let targets = self.remote_targets();
            if !targets.is_empty() {
                deps.poll_remote(&targets);
            }
        }
        for r in deps.take_remote() {
            if r.fetched {
                deps.write_metrics(r.job_id, &r.snapshot);
            }
            self.remotes.insert(r.job_id, r.snapshot);
        }
    }

    /// `walltime_warn` / `walltime_red`, once per crossing: re-armed when an
    /// extension lifts the remaining time back above the line.
    fn check_walltime_events(&mut self, now: i64, remaining: i64, deps: &mut dyn Deps) {
        if remaining > WALLTIME_WARN_SECS {
            self.warned = false;
        } else if !self.warned {
            self.warned = true;
            deps.emit_event(&Event::at(now, "walltime_warn").with("remaining", remaining));
        }
        if remaining > WALLTIME_RED_SECS {
            self.red_warned = false;
        } else if !self.red_warned {
            self.red_warned = true;
            deps.emit_event(&Event::at(now, "walltime_red").with("remaining", remaining));
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

        self.emit_started(now, deps);

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
            let over = deps.read_quota().filter(|q| q.over);
            if let Some(q) = &over {
                if !self.quota_over {
                    let ev = Event::at(now, "quota_over")
                        .with("over_kb", q.over_kb)
                        .with("hard_kb", q.hard_kb);
                    deps.emit_event(&ev);
                }
            }
            self.quota_over = over.is_some();
            self.quota_notice = over.map(|q| notices::quota_notice(q.over_kb, q.hard_kb));
        }

        // Claude Code hint, at the scheduler-poll cadence (one pgrep per
        // poll), not the tick.
        if now - self.hint_checked >= poll {
            self.hint_checked = now;
            self.hint_wanted = deps.claude_hint_wanted();
        }

        if now - self.jobs_checked >= JOBS_EVERY {
            self.jobs_checked = now;
            self.refresh_queue(now, deps);
        }

        // Host monitoring: this node on its own tick, the other jobs'
        // nodes in the background.
        self.sample_local(now, deps);
        self.poll_remotes(now, deps);

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
        self.check_walltime_events(now, remaining, deps);

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
pub fn jobs_summary(rows: &[QueueJob], self_id: u64) -> String {
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

/// [`QueueJob`]s from squeue rows plus the `%i|%j` name lookup.
pub fn queue_jobs(rows: &[JobRow], names: &HashMap<u64, String>) -> Vec<QueueJob> {
    rows.iter()
        .map(|r| QueueJob {
            job_id: r.job_id,
            state: r.state.clone(),
            node: r.node.clone(),
            name: names.get(&r.job_id).cloned(),
        })
        .collect()
}

/// Parse `squeue --me -h -o '%i|%j'` output into id → name.
pub fn parse_job_names(output: &str) -> HashMap<u64, String> {
    output
        .lines()
        .filter_map(|l| {
            let (id, name) = l.trim().split_once('|')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            Some((id.trim().parse().ok()?, name.to_string()))
        })
        .collect()
}

/// The first node of a squeue `%N` nodelist: `c3cpu-a2-u[3-4]` →
/// `c3cpu-a2-u3`, `n[001-004],m7` → `n001`, `node01` → `node01`. `None`
/// when empty (a pending job).
pub fn first_node(nodelist: &str) -> Option<String> {
    let s = nodelist.trim();
    if s.is_empty() {
        return None;
    }
    // The first top-level element: a comma outside brackets ends it.
    let mut depth = 0usize;
    let mut first = s;
    for (i, ch) in s.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                first = &s[..i];
                break;
            }
            _ => {}
        }
    }
    let Some((prefix, rest)) = first.split_once('[') else {
        return Some(first.to_string());
    };
    let (range, suffix) = rest.split_once(']').unwrap_or((rest, ""));
    let head = range
        .split(',')
        .next()
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("");
    Some(format!("{prefix}{head}{suffix}"))
}

/// `12` / `1.5`: MB as G without the unit.
fn gig(mb: u64) -> String {
    let g = mb as f64 / 1024.0;
    if g >= 10.0 {
        format!("{g:.0}")
    } else {
        format!("{g:.1}")
    }
}

/// The bar's load field: `cpu 34% 12/32G` (CPU% of the scope, memory used
/// over the allocation).
pub fn load_line(snap: &Snapshot) -> String {
    let alloc = snap.scope.mem_alloc_mb.unwrap_or(snap.mem.total_mb);
    format!(
        "cpu {}% {}/{}G",
        snap.cpu.pct.round().clamp(0.0, 100.0) as u8,
        gig(snap.mem.used_mb),
        gig(alloc)
    )
}

/// The bar's GPU field: `gpu0 87% 31/40G` for one GPU, `gpu0 87% · gpu1
/// 12%` for several (at most two shown), empty without any.
pub fn gpu_line(snap: &Snapshot) -> String {
    match snap.gpus.as_slice() {
        [] => String::new(),
        [g] => format!(
            "gpu{} {}% {}/{}G",
            g.index,
            g.util_pct,
            gig(g.mem_used_mb),
            gig(g.mem_total_mb)
        ),
        gpus => gpus
            .iter()
            .take(2)
            .map(|g| format!("gpu{} {}%", g.index, g.util_pct))
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// Whether the Claude Code integration is installed under `dir`
/// (`$CLAUDE_CONFIG_DIR`, else `~/.claude`): the session-context hook
/// exists and one of the settings files mentions it (script line 1244).
pub fn claude_integration_active(dir: &Path) -> bool {
    // The native hook (`sinteractive hook session-start`) or the 0.x script
    // registered in either settings file counts; a string test, not JSON
    // parsing, on purpose (the file is the user's).
    ["settings.json", "settings.local.json"].iter().any(|f| {
        std::fs::read_to_string(dir.join(f))
            .map(|s| {
                s.contains("sinteractive hook session-start")
                    || s.contains("sinteractive-session-context")
            })
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

/// The background thread that samples other jobs' nodes: one request
/// (a target list) at a time, results streamed back as they finish. The
/// request channel holds one entry, so a round that is still running makes
/// the next request a no-op instead of a backlog.
struct RemotePoller {
    req: mpsc::SyncSender<Vec<RemoteTarget>>,
    res: mpsc::Receiver<RemoteSnapshot>,
}

impl RemotePoller {
    fn start(state: StateDir, exe: PathBuf) -> Self {
        let (req, req_rx) = mpsc::sync_channel::<Vec<RemoteTarget>>(1);
        let (res_tx, res) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("remote-poll".into())
            .spawn(move || {
                for targets in req_rx {
                    for r in fetch_remote(&state, &exe, &targets) {
                        if res_tx.send(r).is_err() {
                            return;
                        }
                    }
                }
            });
        RemotePoller { req, res }
    }

    fn request(&self, targets: Vec<RemoteTarget>) {
        let _ = self.req.try_send(targets);
    }

    fn take(&self) -> Vec<RemoteSnapshot> {
        self.res.try_iter().collect()
    }
}

/// One round: a target's own fresh `<id>.metrics.json` is used as is;
/// the rest get `ssh NODE <exe> snapshot --json --job ID`, all spawned at
/// once, with [`REMOTE_TIMEOUT`] for the lot. A target that fails (no ssh
/// access, no binary there, timeout) is simply absent from the result.
fn fetch_remote(state: &StateDir, exe: &Path, targets: &[RemoteTarget]) -> Vec<RemoteSnapshot> {
    let now = now_epoch();
    let mut out = Vec::new();
    let mut pending: Vec<(u64, Child, std::fs::File)> = Vec::new();
    for t in targets {
        if let Some(snap) = metrics::read_snapshot(state, t.job_id) {
            if (snap.age_secs(now) as i64) <= REMOTE_CACHE_FRESH {
                out.push(RemoteSnapshot {
                    job_id: t.job_id,
                    snapshot: snap,
                    fetched: false,
                });
                continue;
            }
        }
        let remote = format!(
            "{} snapshot --json --job {}",
            shell_quote(&exe.to_string_lossy()),
            t.job_id
        );
        // stdout goes to a file, not a pipe: nothing to drain while the
        // children run, however long a process list they print.
        let Ok(file) = tempfile::tempfile() else {
            continue;
        };
        let Ok(clone) = file.try_clone() else {
            continue;
        };
        let child = ssh_batch(&t.node, 5, &remote)
            .stdout(Stdio::from(clone))
            .stderr(Stdio::null())
            .spawn();
        if let Ok(child) = child {
            pending.push((t.job_id, child, file));
        }
    }

    let deadline = Instant::now() + REMOTE_TIMEOUT;
    let mut done: Vec<(u64, bool, std::fs::File)> = Vec::new();
    while !pending.is_empty() {
        let mut i = 0;
        while i < pending.len() {
            match pending[i].1.try_wait() {
                Ok(Some(status)) => {
                    let (id, _, file) = pending.remove(i);
                    done.push((id, status.success(), file));
                }
                Ok(None) => i += 1,
                Err(_) => {
                    pending.remove(i);
                }
            }
        }
        if pending.is_empty() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for (_, mut child, _) in pending {
        let _ = child.kill();
        let _ = child.wait();
    }
    for (job_id, ok, mut file) in done {
        if !ok {
            continue;
        }
        let mut text = String::new();
        if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_string(&mut text).is_err() {
            continue;
        }
        if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&text) {
            out.push(RemoteSnapshot {
                job_id,
                snapshot,
                fetched: true,
            });
        }
    }
    out
}

/// `squeue --me -h -o '%i|%j'` → id → job name; empty when squeue fails.
fn job_names(slurm: &Slurm) -> HashMap<u64, String> {
    slurm
        .run("squeue", &["--me", "-h", "-o", "%i|%j"])
        .map(|out| parse_job_names(&out))
        .unwrap_or_default()
}

/// The real side effects.
struct NodeDeps<'a> {
    /// Session name from the last Comment read (see `Deps::current_name`).
    seen_name: Option<Option<String>>,
    ctx: &'a Ctx,
    zellij: &'a ZellijEnv,
    job_id: u64,
    user: String,
    uid: u32,
    claude_dir: std::path::PathBuf,
    sampler: Sampler,
    remote: RemotePoller,
}

impl Deps for NodeDeps<'_> {
    fn current_name(&mut self) -> Option<Option<String>> {
        self.seen_name.clone()
    }

    fn query_end_epoch(&mut self) -> Option<i64> {
        let row = self.ctx.slurm.job(self.job_id).ok()??;
        self.seen_name = Some(sint_core::session::parse_comment(&row.comment).flatten());
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

    fn other_jobs(&mut self) -> Option<Vec<QueueJob>> {
        let rows = self.ctx.slurm.my_jobs(&["RUNNING", "PENDING"]).ok()?;
        let names = job_names(&self.ctx.slurm);
        Some(queue_jobs(&rows, &names))
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

    fn sample_local(&mut self) -> Option<Snapshot> {
        Some(self.sampler.sample())
    }

    fn write_metrics(&mut self, job_id: u64, snap: &Snapshot) {
        if let Err(e) = metrics::write_snapshot(&self.ctx.state, job_id, snap) {
            eprintln!("sinteractive: metrics file: {e}");
        }
    }

    fn remove_metrics(&mut self, job_id: u64) {
        let _ = std::fs::remove_file(self.ctx.state.metrics_file(job_id));
    }

    fn poll_remote(&mut self, targets: &[RemoteTarget]) {
        self.remote.request(targets.to_vec());
    }

    fn take_remote(&mut self) -> Vec<RemoteSnapshot> {
        self.remote.take()
    }

    fn emit_event(&mut self, event: &Event) {
        if let Err(e) = events::append(&self.ctx.state, self.job_id, event) {
            eprintln!("sinteractive: events file: {e}");
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
    let exe = std::env::current_exe().unwrap_or_else(|_| zellij.exe.clone());
    let mut deps = NodeDeps {
        seen_name: None,
        ctx: &ctx,
        zellij: &zellij,
        job_id,
        user,
        uid,
        claude_dir,
        sampler: Sampler::for_current_job(),
        remote: RemotePoller::start(ctx.state.clone(), exe),
    };
    let mut lp = JobLoop::new(lcfg);
    let mut ended_by_us = false;
    let mut reason = "gone";
    while !signalled() {
        match lp.step(now_epoch(), &mut deps) {
            Step::Continue(d) => sleep_interruptible(wait(d)),
            Step::Ending => {
                end_session(&zellij);
                ended_by_us = true;
                reason = "walltime";
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
        reason = "signal";
        kill_session(&zellij);
    }
    deps.emit_event(
        &Event::new("ended")
            .with("job", job_id)
            .with("reason", reason),
    );

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
    use sint_core::metrics::{Cpu, Gpu, GpuProc, Mem, Scope};

    #[derive(Default)]
    struct Fake {
        end: Option<i64>,
        queries: usize,
        poke: bool,
        quota: Option<QuotaSnapshot>,
        hint: bool,
        /// `None` = squeue failed.
        queue: Option<Vec<QueueJob>>,
        alive: bool,
        states: Vec<StateFile>,
        notices: Vec<Vec<Notice>>,
        sent: Vec<StatusMsg>,
        /// What `sample_local` returns.
        local: Option<Snapshot>,
        samples: usize,
        metrics: Vec<(u64, Snapshot)>,
        removed: Vec<u64>,
        remote_requests: Vec<Vec<RemoteTarget>>,
        /// Handed back on the next `take_remote`.
        remote_ready: Vec<RemoteSnapshot>,
        events: Vec<Event>,
    }

    impl Fake {
        fn alive(end: Option<i64>) -> Self {
            Fake {
                end,
                alive: true,
                queue: Some(Vec::new()),
                ..Default::default()
            }
        }

        fn kinds(&self) -> Vec<&str> {
            self.events.iter().map(|e| e.kind.as_str()).collect()
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
        fn other_jobs(&mut self) -> Option<Vec<QueueJob>> {
            self.queue.clone()
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
        fn sample_local(&mut self) -> Option<Snapshot> {
            self.samples += 1;
            self.local.clone()
        }
        fn write_metrics(&mut self, job_id: u64, snap: &Snapshot) {
            self.metrics.push((job_id, snap.clone()));
        }
        fn remove_metrics(&mut self, job_id: u64) {
            self.removed.push(job_id);
        }
        fn poll_remote(&mut self, targets: &[RemoteTarget]) {
            self.remote_requests.push(targets.to_vec());
        }
        fn take_remote(&mut self) -> Vec<RemoteSnapshot> {
            std::mem::take(&mut self.remote_ready)
        }
        fn emit_event(&mut self, e: &Event) {
            self.events.push(e.clone());
        }
    }

    const T0: i64 = 1_800_000_000;

    fn qjob(id: u64, state: &str, node: &str, name: Option<&str>) -> QueueJob {
        QueueJob {
            job_id: id,
            state: state.into(),
            node: node.into(),
            name: name.map(str::to_string),
        }
    }

    fn snap(ts: i64, cpu: f32, used_mb: u64) -> Snapshot {
        Snapshot {
            host: "node01".into(),
            ts,
            scope: Scope {
                job_id: Some(4242),
                cpus_alloc: Some(8),
                mem_alloc_mb: Some(32 * 1024),
                ..Default::default()
            },
            cpu: Cpu {
                pct: cpu,
                ncpu: 64,
                load1: 2.0,
                ..Default::default()
            },
            mem: Mem {
                total_mb: 32 * 1024,
                used_mb,
            },
            ..Default::default()
        }
    }

    fn gpu(index: u32, util: u8, held: bool) -> Gpu {
        Gpu {
            index,
            name: "A100".into(),
            util_pct: util,
            mem_used_mb: 31 * 1024,
            mem_total_mb: 40 * 1024,
            procs: if held {
                vec![GpuProc {
                    pid: 1,
                    mem_mb: 100,
                    sm_pct: None,
                }]
            } else {
                Vec::new()
            },
            ..Default::default()
        }
    }

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
        f.queue = Some(vec![
            qjob(4242, "RUNNING", "node01", None),
            qjob(1, "RUNNING", "n2", None),
            qjob(2, "RUNNING", "n3", None),
            qjob(3, "PENDING", "", None),
        ]);
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
        let row = |id: u64, state: &str| QueueJob {
            job_id: id,
            state: state.into(),
            ..QueueJob::default()
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
    fn started_once_and_walltime_events_once_per_crossing() {
        let end = T0 + 10_000;
        let mut f = Fake::alive(Some(end));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        lp.step(T0 + 1, &mut f);
        assert_eq!(f.kinds(), ["started"]);
        let st = &f.events[0];
        assert_eq!(st.ts, T0);
        assert_eq!(st.field_i64("job"), Some(4242));
        assert_eq!(st.field_str("node"), Some("node01"));
        assert_eq!(st.field_str("name"), Some("t"));

        // 30 min left: warn, once.
        lp.step(end - 1800, &mut f);
        lp.step(end - 1799, &mut f);
        assert_eq!(f.kinds(), ["started", "walltime_warn"]);
        assert_eq!(f.events[1].field_i64("remaining"), Some(1800));
        // 10 min left: red, once (the red-phase re-check happens too).
        lp.step(end - 600, &mut f);
        lp.step(end - 599, &mut f);
        assert_eq!(f.kinds(), ["started", "walltime_warn", "walltime_red"]);
        assert_eq!(f.events[2].field_i64("remaining"), Some(600));

        // An extension lifts it back above both lines and re-arms them.
        f.end = Some(end + 7200);
        lp.step(end - 570, &mut f); // the poll-cadence re-query sees it
        assert_eq!(f.events.len(), 3);
        lp.step(end + 7200 - 1700, &mut f);
        assert_eq!(f.kinds().last(), Some(&"walltime_warn"));
    }

    #[test]
    fn job_done_when_another_job_leaves_the_queue() {
        let mut f = Fake::alive(Some(T0 + 7200));
        f.queue = Some(vec![
            qjob(4242, "RUNNING", "node01", Some("sint-t")),
            qjob(9001, "RUNNING", "n2", Some("train")),
            qjob(9002, "PENDING", "", None),
        ]);
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        assert_eq!(f.kinds(), ["started"]);
        assert_eq!(f.sent.last().unwrap().jobs, "1R 1PD");

        // A squeue failure in between changes nothing.
        f.queue = None;
        lp.step(T0 + JOBS_EVERY, &mut f);
        assert_eq!(f.kinds(), ["started"]);
        assert_eq!(f.sent.last().unwrap().jobs, "1R 1PD");

        // 9001 finished; 9002 is still pending.
        f.queue = Some(vec![
            qjob(4242, "RUNNING", "node01", Some("sint-t")),
            qjob(9002, "PENDING", "", None),
        ]);
        lp.step(T0 + 2 * JOBS_EVERY, &mut f);
        assert_eq!(f.kinds(), ["started", "job_done"]);
        let done = f.events.last().unwrap();
        assert_eq!(done.ts, T0 + 2 * JOBS_EVERY);
        assert_eq!(done.field_i64("job"), Some(9001));
        assert_eq!(done.field_str("name"), Some("train"));
        assert_eq!(f.sent.last().unwrap().jobs, "1PD");

        // Then 9002 as well (unnamed → null).
        f.queue = Some(vec![qjob(4242, "RUNNING", "node01", Some("sint-t"))]);
        lp.step(T0 + 3 * JOBS_EVERY, &mut f);
        assert_eq!(f.kinds(), ["started", "job_done", "job_done"]);
        let done = f.events.last().unwrap();
        assert_eq!(done.field_i64("job"), Some(9002));
        assert_eq!(done.fields.get("name"), Some(&serde_json::Value::Null));
        assert_eq!(f.sent.last().unwrap().jobs, "");
    }

    #[test]
    fn quota_over_once_per_episode() {
        let mut f = Fake::alive(Some(T0 + 7200));
        f.quota = Some(quota::snapshot("tester", 2048, 1024, T0));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        assert_eq!(f.kinds(), ["started", "quota_over"]);
        assert_eq!(f.events[1].field_i64("over_kb"), Some(1024));
        assert_eq!(f.events[1].field_i64("hard_kb"), Some(1024));
        // Still over at the next check: nothing new.
        f.poke = true;
        lp.step(T0 + 6, &mut f);
        assert_eq!(f.events.len(), 2);
        // Cleared, then over again: a second episode.
        f.quota = Some(quota::snapshot("tester", 512, 1024, T0));
        f.poke = true;
        lp.step(T0 + 12, &mut f);
        f.quota = Some(quota::snapshot("tester", 4096, 1024, T0));
        f.poke = true;
        lp.step(T0 + 18, &mut f);
        assert_eq!(f.kinds(), ["started", "quota_over", "quota_over"]);
    }

    #[test]
    fn local_sampling_feeds_the_bar_and_the_metrics_file() {
        let mut f = Fake::alive(Some(T0 + 7200));
        f.local = Some(snap(T0, 34.4, 12 * 1024));
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        assert_eq!(f.samples, 1);
        let m = f.sent.last().unwrap();
        assert_eq!(m.load, "cpu 34% 12/32G");
        assert_eq!(m.gpu, "");
        assert_eq!(m.hosts.len(), 1);
        assert_eq!(m.hosts[0].host, "node01");
        assert_eq!(m.hosts[0].job_id, 4242);
        assert_eq!(m.hosts[0].job_name.as_deref(), Some("t"));
        assert_eq!(m.hosts[0].cpu_pct, 34);
        assert_eq!(m.hosts[0].cpu_alloc, 8);
        assert_eq!(m.hosts[0].mem_alloc_mb, 32 * 1024);
        assert_eq!(m.hosts[0].age_secs, 0);
        // Written on the first tick, then every METRICS_EVERY.
        assert_eq!(f.metrics.len(), 1);
        assert_eq!(f.metrics[0].0, 4242);

        // Sampled every SAMPLE_EVERY, not every tick.
        lp.step(T0 + 1, &mut f);
        assert_eq!(f.samples, 1);
        lp.step(T0 + 2, &mut f);
        assert_eq!(f.samples, 2);
        assert_eq!(f.metrics.len(), 1);
        lp.step(T0 + 5, &mut f);
        assert_eq!(f.metrics.len(), 2);
        assert_eq!(
            f.sent.last().unwrap().hosts[0].age_secs,
            5,
            "sample ts is T0"
        );

        // GPUs: one shows memory, two show utilisation only.
        let mut one = snap(T0 + 7, 50.0, 1536);
        one.gpus = vec![gpu(0, 87, true)];
        f.local = Some(one);
        lp.step(T0 + 7, &mut f);
        let m = f.sent.last().unwrap();
        assert_eq!(m.load, "cpu 50% 1.5/32G");
        assert_eq!(m.gpu, "gpu0 87% 31/40G");
        assert_eq!(m.hosts[0].gpus.len(), 1);
        let mut three = snap(T0 + 9, 50.0, 1536);
        three.gpus = vec![gpu(0, 87, true), gpu(1, 12, false), gpu(2, 3, false)];
        f.local = Some(three);
        lp.step(T0 + 9, &mut f);
        assert_eq!(f.sent.last().unwrap().gpu, "gpu0 87% · gpu1 12%");
    }

    #[test]
    fn gpu_idle_after_ten_minutes_held_and_quiet() {
        let mut f = Fake::alive(Some(T0 + 20_000));
        let mut lp = JobLoop::new(cfg());
        let at = |lp: &mut JobLoop, f: &mut Fake, t: i64, util: u8, held: bool| {
            let mut s = snap(t, 10.0, 1024);
            s.gpus = vec![gpu(0, util, held), gpu(1, 90, true)];
            f.local = Some(s);
            lp.step(t, f);
        };
        at(&mut lp, &mut f, T0, 2, true);
        at(&mut lp, &mut f, T0 + 300, 1, true);
        at(&mut lp, &mut f, T0 + 598, 0, true);
        assert_eq!(f.kinds(), ["started"], "not yet ten minutes");
        at(&mut lp, &mut f, T0 + 600, 0, true);
        assert_eq!(f.kinds(), ["started", "gpu_idle"]);
        let ev = f.events.last().unwrap();
        assert_eq!(ev.field_i64("gpu"), Some(0));
        assert_eq!(ev.field_i64("util_pct"), Some(0));
        assert_eq!(ev.field_i64("idle_secs"), Some(600));
        // Still idle: no repeat inside the episode.
        at(&mut lp, &mut f, T0 + 1200, 0, true);
        assert_eq!(f.events.len(), 2);
        // Busy again resets; released (no process) never counts.
        at(&mut lp, &mut f, T0 + 1202, 50, true);
        at(&mut lp, &mut f, T0 + 1204, 0, false);
        at(&mut lp, &mut f, T0 + 1900, 0, false);
        assert_eq!(f.events.len(), 2);
        at(&mut lp, &mut f, T0 + 1902, 0, true);
        at(&mut lp, &mut f, T0 + 2502, 0, true);
        assert_eq!(f.kinds(), ["started", "gpu_idle", "gpu_idle"]);
    }

    #[test]
    fn other_running_jobs_are_polled_and_listed_as_hosts() {
        let mut f = Fake::alive(Some(T0 + 7200));
        f.local = Some(snap(T0, 10.0, 1024));
        f.queue = Some(vec![
            qjob(4242, "RUNNING", "node01", Some("sint-t")),
            qjob(9001, "RUNNING", "c3cpu-a2-u[3-4]", Some("train")),
            qjob(9003, "RUNNING", "node01", Some("same-node")),
            qjob(9002, "PENDING", "", None),
        ]);
        let mut lp = JobLoop::new(cfg());
        lp.step(T0, &mut f);
        // One request on the first tick: the running job on another node
        // only (the one sharing this node is sampled by our own scope).
        assert_eq!(
            f.remote_requests,
            vec![vec![RemoteTarget {
                job_id: 9001,
                node: "c3cpu-a2-u3".into()
            }]]
        );
        lp.step(T0 + 5, &mut f);
        assert_eq!(f.remote_requests.len(), 1, "not before REMOTE_EVERY");
        lp.step(T0 + 10, &mut f);
        assert_eq!(f.remote_requests.len(), 2);

        // A fetched sample is written out and shown after our own host.
        let mut rs = snap(T0 + 11, 75.0, 2048);
        rs.host = "c3cpu-a2-u3".into();
        f.remote_ready = vec![RemoteSnapshot {
            job_id: 9001,
            snapshot: rs,
            fetched: true,
        }];
        lp.step(T0 + 12, &mut f);
        assert!(f
            .metrics
            .iter()
            .any(|(id, s)| *id == 9001 && s.host == "c3cpu-a2-u3"));
        let m = f.sent.last().unwrap();
        assert_eq!(m.hosts.len(), 2);
        assert_eq!(m.hosts[1].host, "c3cpu-a2-u3");
        assert_eq!(m.hosts[1].job_id, 9001);
        assert_eq!(m.hosts[1].job_name.as_deref(), Some("train"));
        assert_eq!(m.hosts[1].cpu_pct, 75);

        // One read from that job's own file is shown but not re-written.
        let n = f.metrics.len();
        f.remote_ready = vec![RemoteSnapshot {
            job_id: 9001,
            snapshot: snap(T0 + 13, 20.0, 2048),
            fetched: false,
        }];
        lp.step(T0 + 13, &mut f);
        assert_eq!(f.metrics.len(), n);
        assert_eq!(f.sent.last().unwrap().hosts[1].cpu_pct, 20);

        // The job leaves: host dropped, its file removed, job_done.
        f.queue = Some(vec![
            qjob(4242, "RUNNING", "node01", Some("sint-t")),
            qjob(9003, "RUNNING", "node01", Some("same-node")),
            qjob(9002, "PENDING", "", None),
        ]);
        lp.step(T0 + JOBS_EVERY, &mut f);
        assert_eq!(f.removed, vec![9001]);
        assert_eq!(f.sent.last().unwrap().hosts.len(), 1);
        assert_eq!(f.kinds(), ["started", "job_done"]);
        // Nothing left to poll on other nodes: no further requests.
        lp.step(T0 + 40, &mut f);
        assert_eq!(f.remote_requests.len(), 2, "{:?}", f.remote_requests);
        assert!(f
            .remote_requests
            .iter()
            .all(|r| r.len() == 1 && r[0].job_id == 9001));
    }

    #[test]
    fn nodelist_first_node() {
        assert_eq!(first_node("node01").as_deref(), Some("node01"));
        assert_eq!(
            first_node("c3cpu-a2-u[3-4]").as_deref(),
            Some("c3cpu-a2-u3")
        );
        assert_eq!(first_node("n[001-004],m7").as_deref(), Some("n001"));
        assert_eq!(first_node("n[7,9-11]").as_deref(), Some("n7"));
        assert_eq!(first_node("a3,b[1-2]").as_deref(), Some("a3"));
        assert_eq!(first_node("gpu[2]x").as_deref(), Some("gpu2x"));
        assert_eq!(first_node(""), None);
        assert_eq!(first_node("  "), None);
    }

    #[test]
    fn job_name_lookup_parses_squeue_pairs() {
        let names = parse_job_names(
            "1|train
2|sint-web

3|
bad|x
4 | spaced 
",
        );
        assert_eq!(names.get(&1).map(String::as_str), Some("train"));
        assert_eq!(names.get(&2).map(String::as_str), Some("sint-web"));
        assert_eq!(names.get(&3), None);
        assert_eq!(names.get(&4).map(String::as_str), Some("spaced"));
        assert_eq!(names.len(), 3);
        let rows = vec![JobRow {
            job_id: 1,
            state: "RUNNING".into(),
            node: "n1".into(),
            ..JobRow::default()
        }];
        let q = queue_jobs(&rows, &names);
        assert_eq!(q, vec![qjob(1, "RUNNING", "n1", Some("train"))]);
    }

    #[test]
    fn bar_lines() {
        let mut s = snap(T0, 99.6, 512);
        assert_eq!(load_line(&s), "cpu 100% 0.5/32G");
        s.scope.mem_alloc_mb = None;
        s.mem.total_mb = 250 * 1024;
        assert_eq!(load_line(&s), "cpu 100% 0.5/250G");
        assert_eq!(gpu_line(&s), "");
        s.gpus = vec![gpu(3, 5, false)];
        assert_eq!(gpu_line(&s), "gpu3 5% 31/40G");
        s.gpus = vec![
            gpu(0, 1, false),
            gpu(1, 2, false),
            gpu(2, 3, false),
            gpu(3, 4, false),
        ];
        assert_eq!(gpu_line(&s), "gpu0 1% · gpu1 2%");
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
