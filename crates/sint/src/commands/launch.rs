//! `sinteractive [launch]` — submit a session job, wait for it, attach.
//!
//! Ports `main` (script lines 510-856): nesting guard, name validation and
//! duplicate check, default sbatch flags from [`Config`], the maintenance
//! fit, the job-limit check, submission with the running binary as the
//! batch script (`… <exe> __job …`), Comment tagging, the pending wait,
//! the readiness wait over one ssh, then either the detached report or an
//! interactive attach followed by the teardown summary.
//!
//! Ctrl-C semantics: until the session is up an interrupt cancels the job
//! (the 0.x `cleanup` trap); once it is up a detach or disconnect must not.
//!
//! Undocumented knobs for the tests: `SINTERACTIVE_POLL_FAST=<secs>`
//! replaces every wait in this file (scheduler poll, spinner, readiness
//! probe, post-detach grace), and `SINTERACTIVE_RUNTIME_DIR` (default
//! `/tmp`) is where the node-side readiness marker lives.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use sint_core::color::Palette;
use sint_core::config::Config;
use sint_core::joblimit::{self, LimitHit};
use sint_core::maint::{self, Fit};
use sint_core::notices::{self, format_local_datetime};
use sint_core::now_epoch;
use sint_core::session::{comment_for, parse_comment, validate_name, SessionInfo};
use sint_core::slurm::squeue::JobRow;
use sint_core::slurm::{Slurm, SlurmError};
use sint_core::theme::is_tty;
use sint_core::time::{
    format_short_duration, parse_time, seconds_to_slurm_time, slurm_time_to_seconds,
    slurm_timestamp_to_epoch,
};

use super::common::{current_exe, pend_reason, print_json, render_status, session_table_line, Ctx};
use crate::cli::LaunchArgs;
use crate::zellij_cmd::shell_quote;

pub fn run(args: LaunchArgs) -> Result<i32> {
    launch(&Ctx::new(), args, None)
}

/// Set by the SIGINT/SIGTERM handler while the job is not yet up.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Arm the interrupt trap: from here until [`disarm`] a Ctrl-C cancels the
/// job instead of killing us mid-wait and leaving it in the queue.
fn arm() {
    INTERRUPTED.store(false, Ordering::SeqCst);
    // SAFETY: installing a handler that only stores to an atomic.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
    }
}

/// The session is up: restore default signal handling (script line 770).
fn disarm() {
    // SAFETY: restoring the default disposition.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }
}

fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Sleep `d` in slices so an interrupt is noticed promptly.
fn sleep_interruptible(d: Duration) {
    let end = Instant::now() + d;
    while !interrupted() {
        let now = Instant::now();
        if now >= end {
            break;
        }
        std::thread::sleep((end - now).min(Duration::from_millis(100)));
    }
}

/// `SINTERACTIVE_POLL_FAST` as a duration, when set.
fn fast_poll() -> Option<Duration> {
    let v = std::env::var("SINTERACTIVE_POLL_FAST").ok()?;
    v.trim().parse::<f64>().ok().map(Duration::from_secs_f64)
}

/// `SINTERACTIVE_RUNTIME_DIR` (default `/tmp`): where `__job` writes the
/// readiness marker on the node.
fn runtime_dir() -> String {
    std::env::var("SINTERACTIVE_RUNTIME_DIR")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/tmp".to_string())
}

/// The node-side readiness probe, run over one ssh: poll for the marker
/// `__job` creates once the session is up, 30 × `sleep` (script line 759).
/// Phase 2 swaps the `test -e` for whatever the multiplexer offers.
fn readiness_probe(job_id: u64, sleep: &str) -> String {
    format!(
        "for i in $(seq 1 30); do test -e '{dir}/sint-{job_id}/ready' && exit 0; sleep {sleep}; done; exit 1",
        dir = runtime_dir()
    )
}

/// Local wall-clock rendering of an epoch (`date -d @EPOCH +FMT`).
fn fmt_local(epoch: i64, fmt: &[time::format_description::BorrowedFormatItem<'_>]) -> String {
    let off = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    time::OffsetDateTime::from_unix_timestamp(epoch)
        .map(|t| t.to_offset(off))
        .ok()
        .and_then(|t| t.format(fmt).ok())
        .unwrap_or_else(|| epoch.to_string())
}

/// Whether `args` already carries one of `flags` (`--flag`, `--flag=…`, or a
/// short `-x…`), as the 0.x `grep -qE '^(--time|-t)'` prefix tests did.
fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|a| {
        flags.iter().any(|f| {
            if f.starts_with("--") {
                a == f || a.starts_with(&format!("{f}=")) || (*f == "--mem" && a.starts_with(f))
            } else {
                a.starts_with(f)
            }
        })
    })
}

/// `--flag=V` / `--flag V` / `-x V` for the first of `long`/`short` present.
fn flag_value(args: &[String], long: &str, short: Option<&str>) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if let Some(v) = a.strip_prefix(long).and_then(|r| r.strip_prefix('=')) {
            return Some(v.to_string());
        }
        if a == long || short.is_some_and(|s| a == s) {
            return args.get(i + 1).cloned();
        }
        i += 1;
    }
    None
}

/// Translate the clap launch flags to sbatch options (script lines 366-431).
fn own_sbatch_args(args: &LaunchArgs, warn: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(node) = &args.node {
        out.push(format!("--nodelist={node}"));
    }
    if let Some(t) = &args.time {
        out.push(format!("--time={}", parse_time_warning(t, warn)));
    }
    if let Some(p) = &args.partition {
        out.push(format!("--partition={p}"));
    }
    if let Some(n) = args.threads {
        out.push(format!("--cpus-per-task={n}"));
    }
    if let Some(m) = &args.mem {
        out.push(format!("--mem={m}"));
    }
    out
}

/// [`parse_time`], collecting the pass-through warning instead of failing.
fn parse_time_warning(input: &str, warn: &mut Vec<String>) -> String {
    match parse_time(input) {
        Ok(t) => t,
        Err(w) => {
            warn.push(w);
            input.to_string()
        }
    }
}

/// Prepend the `SINTERACTIVE_*` defaults the user did not override
/// (script lines 543-573). Explicit flags always win: sbatch takes the last
/// occurrence, and these go first.
fn apply_defaults(cfg: &Config, args: &mut Vec<String>, warn: &mut Vec<String>) {
    let mut defaults = Vec::new();
    if !has_flag(args, &["--time", "-t"]) {
        defaults.push(format!("--time={}", parse_time_warning(&cfg.time, warn)));
    }
    if !has_flag(args, &["--partition", "-p"]) {
        defaults.push(format!("--partition={}", cfg.partition));
    }
    if let Some(qos) = &cfg.qos {
        if !has_flag(args, &["--qos", "-q"]) {
            defaults.push(format!("--qos={qos}"));
        }
    }
    if !has_flag(args, &["--cpus-per-task", "-c"]) {
        defaults.push(format!("--cpus-per-task={}", cfg.cpus));
    }
    if !has_flag(args, &["--mem"]) {
        defaults.push(format!("--mem={}", cfg.mem));
    }
    defaults.append(args);
    *args = defaults;
}

/// The partition the request resolves to (script line 1824).
fn partition_of(args: &[String], cfg: &Config) -> String {
    flag_value(args, "--partition", Some("-p")).unwrap_or_else(|| cfg.partition.clone())
}

/// Outcome of the maintenance fit for the launch narration.
struct MaintCarry {
    name: String,
    ends_epoch: i64,
}

/// Trim `--time` to end before the next MAINT reservation (script lines
/// 2019-2070). Returns the carry for `__job`, or an error message when the
/// gap is too small to be worth a session.
fn fit_maintenance(
    slurm: &Slurm,
    args: &mut [String],
    p: &Palette,
) -> Result<Option<MaintCarry>, String> {
    // An explicit --reservation means the user has arranged to run inside
    // the window; that is theirs to get right.
    if args
        .iter()
        .any(|a| a == "--reservation" || a.starts_with("--reservation="))
    {
        return Ok(None);
    }
    let now = now_epoch();
    let Ok(reservations) = slurm.reservations() else {
        return Ok(None);
    };
    let next = maint::next_maintenance(&reservations, now);
    let Some(time_str) = flag_value(args, "--time", Some("-t")) else {
        return Ok(None);
    };
    let Some(secs) = slurm_time_to_seconds(&time_str) else {
        return Ok(None);
    };
    match maint::fit(secs, now, next.as_ref()) {
        Fit::Unchanged => Ok(None),
        Fit::Trimmed {
            secs,
            reservation,
            ends_epoch,
        } => {
            let fitted = seconds_to_slurm_time(secs);
            maint::replace_time_in_args(args, &fitted);
            let (reset, bold, dim, warn) = (&p.reset, &p.bold, &p.dim, &p.warn);
            let when = format_local_datetime(ends_epoch + maint::MAINT_MARGIN);
            eprintln!(
                "{warn}{bold}!{reset} {warn}Maintenance ({reservation}) starts {when}.{reset}"
            );
            eprintln!(
                "  {dim}Shortened the request from{reset} {time_str} {dim}to{reset} {bold}{fitted}{reset} {dim}so the session ends before it.{reset}"
            );
            eprintln!();
            Ok(Some(MaintCarry {
                name: reservation,
                ends_epoch,
            }))
        }
        Fit::Refuse {
            reservation,
            starts_epoch,
        } => {
            let (reset, bold, dim, err, key, id) =
                (&p.reset, &p.bold, &p.dim, &p.err, &p.key, &p.id);
            let when = format_local_datetime(starts_epoch);
            let mut m = String::new();
            m.push_str(&format!(
                "\n{err}{bold}Error:{reset}{err} maintenance starts too soon to open a session.{reset}\n\n"
            ));
            m.push_str(&format!(
                "  {key}{:<13}{reset} {id}{reservation}{reset}\n",
                "Reservation:"
            ));
            m.push_str(&format!(
                "  {key}{:<13}{reset} {when} {dim}(in {}){reset}\n\n",
                "Starts:",
                format_short_duration(starts_epoch - now)
            ));
            m.push_str(&format!(
                "{dim}Slurm won't schedule a job that runs into the window, and what is{reset}\n\
                 {dim}left before it is under {}. Wait for the reservation to end.{reset}\n\n\
                 {dim}See 'scontrol show reservation' for full details.{reset}\n",
                format_short_duration(maint::MAINT_MIN_SESSION)
            ));
            Err(m)
        }
    }
}

/// `squeue … -o '%i|%j'` for the job-limit table's NAME column; empty when
/// squeue has nothing to say.
fn job_names(slurm: &Slurm, partition: &str) -> HashMap<u64, String> {
    let mut names = HashMap::new();
    let Ok(out) = slurm.run(
        "squeue",
        &[
            "--me",
            "--partition",
            partition,
            "--states",
            "RUNNING,PENDING",
            "--noheader",
            "-o",
            "%i|%j",
        ],
    ) else {
        return names;
    };
    for line in out.lines() {
        if let Some((id, name)) = line.split_once('|') {
            if let Ok(id) = id.trim().parse() {
                names.insert(id, name.trim().to_string());
            }
        }
    }
    names
}

/// The QOS the request resolves to: `--qos`, `SINTERACTIVE_QOS`, else the
/// partition name (0.x hardcoded `interactive` for both).
fn qos_of(args: &[String], cfg: &Config, partition: &str) -> String {
    flag_value(args, "--qos", Some("-q"))
        .or_else(|| cfg.qos.clone())
        .unwrap_or_else(|| partition.to_string())
}

/// Refuse when one more job would exceed the QOS MaxJobsPerUser cap
/// (script lines 2076-2150). Fails open when Slurm cannot answer.
fn check_job_limit(ctx: &Ctx, args: &[String], partition: &str) -> Option<LimitHit> {
    let qos = qos_of(args, &ctx.cfg, partition);
    let limit = ctx.slurm.qos_max_jobs_per_user(&qos)?;
    let rows = ctx.slurm.my_jobs(&["RUNNING", "PENDING"]).ok()?;
    joblimit::check(&qos, Some(limit), &rows, partition)
}

fn print_limit_hit(ctx: &Ctx, hit: &LimitHit, partition: &str, p: &Palette) {
    let (reset, bold, dim, err, key, id, ok, warn) = (
        &p.reset, &p.bold, &p.dim, &p.err, &p.key, &p.id, &p.ok, &p.warn,
    );
    eprintln!();
    eprintln!(
        "{err}{bold}Error:{reset}{err} You already have {}/{} {partition} jobs (the maximum allowed).{reset}",
        hit.jobs.len(),
        hit.limit
    );
    eprintln!();
    let names = job_names(&ctx.slurm, partition);
    eprintln!("{dim}Your current {partition} jobs:{reset}");
    eprintln!();
    eprintln!(
        "  {dim}{:<10}  {:<18}  {:<14}  {:<9}  {:<10}  {:<10}{reset}",
        "JOBID", "NAME", "NODE", "STATE", "ELAPSED", "TIMELIMIT"
    );
    for j in &hit.jobs {
        // A PENDING job holds a slot just as a RUNNING one does, which is
        // the whole reason this table is here — so it is flagged, not hidden.
        let state_c = if j.state == "RUNNING" { ok } else { warn };
        let name = names
            .get(&j.job_id)
            .cloned()
            .or_else(|| {
                parse_comment(&j.comment).map(|n| {
                    n.map(|n| format!("sint-{n}"))
                        .unwrap_or_else(|| "sinteractive".into())
                })
            })
            .unwrap_or_default();
        eprintln!(
            "  {id}{:<10}{reset}  {bold}{:<18}{reset}  {id}{:<14}{reset}  {state_c}{:<9}{reset}  {:<10}  {:<10}",
            j.job_id, name, j.node, j.state, j.elapsed, j.time_limit
        );
    }
    eprintln!();
    eprintln!("{key}To reattach to an existing session:{reset}");
    for j in hit.jobs.iter().filter(|j| j.state == "RUNNING") {
        eprintln!("  sinteractive attach {id}{}{reset}", j.job_id);
    }
    eprintln!();
    eprintln!("{key}To free a slot, cancel a session:{reset}");
    for j in &hit.jobs {
        eprintln!("  scancel {id}{}{reset}", j.job_id);
    }
    eprintln!();
}

/// Heads-up when sessions are already running (script lines 606-626):
/// informational only, the job-limit check is what refuses.
fn note_running_sessions(running: &[JobRow], p: &Palette) {
    let (reset, dim) = (&p.reset, &p.dim);
    match running {
        [] => {}
        [r] => {
            let name = parse_comment(&r.comment).flatten();
            match &name {
                Some(n) => eprintln!(
                    "{dim}Note: you already have a running session named '{n}' (job {} on {}).{reset}",
                    r.job_id, r.node
                ),
                None => eprintln!(
                    "{dim}Note: you already have a running session (job {} on {}).{reset}",
                    r.job_id, r.node
                ),
            }
            let target = name.unwrap_or_else(|| r.job_id.to_string());
            eprintln!(
                "{dim}      Reattach with 'sinteractive attach {target}'; starting a new session...{reset}"
            );
        }
        many => {
            eprintln!(
                "{dim}Note: you already have {} running sessions ('sinteractive list' shows them).{reset}",
                many.len()
            );
            eprintln!("{dim}      Starting a new session...{reset}");
        }
    }
}

fn scancel_quiet(slurm: &Slurm, job_id: u64) {
    let _ = slurm.run("scancel", &["--quiet", &job_id.to_string()]);
}

/// The Ctrl-C-before-the-session-is-up path (0.x `cleanup`): cancel the
/// job and say so, since otherwise it vanishes silently.
fn cancelled_by_user(slurm: &Slurm, job_id: u64, p: &Palette) -> i32 {
    disarm();
    scancel_quiet(slurm, job_id);
    let (reset, dim, ok, id) = (&p.reset, &p.dim, &p.ok, &p.id);
    eprintln!();
    eprintln!("{ok}✓{reset} {dim}Cancelled job{reset} {id}{job_id}{reset}{dim}.{reset}");
    130
}

/// Slurm's estimated start, only when it is ahead of now (it can be `N/A`,
/// or already past just before the job flips to RUNNING).
fn est_start(start_time: &str, now: i64) -> String {
    let Some(start) = slurm_timestamp_to_epoch(start_time) else {
        return String::new();
    };
    if start <= now {
        return String::new();
    }
    let fmt: &[time::format_description::BorrowedFormatItem<'_>] = if start - now < 86_400 {
        time::macros::format_description!("[hour]:[minute]")
    } else {
        time::macros::format_description!("[weekday repr:short] [hour]:[minute]")
    };
    format!(" — est. start {}", fmt_local(start, fmt))
}

/// Outcome of the pending wait.
enum Waited {
    Running,
    Interrupted,
    Gone,
}

/// Poll the scheduler until the job runs (script lines 686-748): every 5 s,
/// with a spinner redrawn in place on a tty, or a dot per poll for logs.
fn wait_for_running(slurm: &Slurm, job_id: u64, p: &Palette) -> Result<Waited> {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let tty = is_tty(2);
    let poll_every = fast_poll().unwrap_or(Duration::from_secs(5));
    let spin_every = fast_poll().unwrap_or(Duration::from_millis(500));
    let (reset, dim, id) = (&p.reset, &p.dim, &p.id);
    let wait_start = now_epoch();
    let mut last_poll: Option<Instant> = None;
    let mut spin = 0usize;
    let mut reason = String::new();
    let mut start_str = String::new();
    let clear = || {
        if tty {
            eprint!("\r\x1b[K");
        }
    };
    loop {
        if interrupted() {
            clear();
            return Ok(Waited::Interrupted);
        }
        if last_poll.is_none_or(|t| t.elapsed() >= poll_every) {
            last_poll = Some(Instant::now());
            let row = match slurm.job(job_id) {
                Ok(row) => row,
                Err(SlurmError::Failed { .. }) if interrupted() => {
                    clear();
                    return Ok(Waited::Interrupted);
                }
                Err(e) => return Err(e.into()),
            };
            match row {
                Some(r) if r.state == "RUNNING" => {
                    clear();
                    return Ok(Waited::Running);
                }
                Some(r) if r.state == "PENDING" => {
                    reason = r.reason;
                    start_str = r.start_time;
                }
                _ => {
                    clear();
                    return Ok(Waited::Gone);
                }
            }
            if !tty {
                eprint!(".");
            }
        }
        if tty {
            let now = now_epoch();
            eprint!(
                "\r\x1b[K {id}{}{reset} {dim}{}{}{reset} {dim}({} elapsed){reset}",
                FRAMES[spin % FRAMES.len()],
                pend_reason(&reason),
                est_start(&start_str, now),
                format_short_duration(now - wait_start)
            );
            spin += 1;
            sleep_interruptible(spin_every);
        } else {
            sleep_interruptible(poll_every);
        }
    }
}

/// Run the readiness probe on `node` over one ssh (script line 758).
fn wait_until_ready(node: &str, job_id: u64) -> bool {
    let sleep = match fast_poll() {
        Some(d) => format!("{}", d.as_secs_f64()),
        None => "1".to_string(),
    };
    Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "StrictHostKeyChecking=accept-new",
            node,
            &readiness_probe(job_id, &sleep),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The launch flow. `created` is `Some(true)` from `ensure`, which reports
/// the field in its JSON; every other caller leaves it off.
pub(crate) fn launch(ctx: &Ctx, args: LaunchArgs, created: Option<bool>) -> Result<i32> {
    let p = ctx.palette(2);
    let (reset, bold, dim, err, key, id, ok) =
        (&p.reset, &p.bold, &p.dim, &p.err, &p.key, &p.id, &p.ok);

    // Don't allow an attaching launch from inside an existing session — it
    // would nest multiplexers. A --detach launch never attaches, so it is
    // allowed from within a session.
    if ctx.inside_session() && !args.detach {
        eprintln!(
            "{err}{bold}Error:{reset}{err} Already inside an sinteractive session. Exit this session first.{reset}"
        );
        return Ok(1);
    }

    let name = args.name.clone().filter(|n| !n.is_empty());
    if let Some(n) = &name {
        if let Err(why) = validate_name(n) {
            eprintln!("{err}{bold}Error:{reset}{err} --{why}{reset}");
            return Ok(1);
        }
        // Best-effort duplicate-name guard: the Comment marker that records
        // the name is only set after submission, so two near-simultaneous
        // launches with the same name can both pass here. Reattach-by-name
        // resolves the resulting ambiguity by erroring on multiple matches.
        let wanted = comment_for(Some(n));
        let existing: Vec<String> = ctx
            .sessions()?
            .iter()
            .filter(|r| r.comment == wanted)
            .map(|r| r.job_id.to_string())
            .collect();
        if !existing.is_empty() {
            eprintln!(
                "{err}{bold}Error:{reset}{err} A sinteractive session named '{n}' is already running:{reset}"
            );
            eprintln!("  {dim}JobID(s):{reset} {id}{}{reset}", existing.join(" "));
            eprintln!(
                "{dim}Pick a different name, or reattach with: sinteractive attach {n}{reset}"
            );
            return Ok(1);
        }
    }

    // sbatch options: ours (translated), then the passthrough, then the
    // defaults in front of both so an explicit flag wins.
    let mut warnings = Vec::new();
    let mut sbatch_args = own_sbatch_args(&args, &mut warnings);
    sbatch_args.extend(args.sbatch_args.iter().cloned());
    if let Some(n) = &name {
        if !has_flag(&sbatch_args, &["--job-name", "-J"]) {
            sbatch_args.insert(0, format!("--job-name=sint-{n}"));
        }
    }
    apply_defaults(&ctx.cfg, &mut sbatch_args, &mut warnings);
    for w in &warnings {
        eprintln!("{dim}Warning: {w}{reset}");
    }

    let maint_carry = match fit_maintenance(&ctx.slurm, &mut sbatch_args, &p) {
        Ok(c) => c,
        Err(msg) => {
            eprint!("{msg}");
            return Ok(1);
        }
    };

    let partition = partition_of(&sbatch_args, &ctx.cfg);
    if let Some(hit) = check_job_limit(ctx, &sbatch_args, &partition) {
        print_limit_hit(ctx, &hit, &partition, &p);
        return Ok(1);
    }

    note_running_sessions(&ctx.running_sessions()?, &p);

    // The batch job body: this binary's `__job` verb with the feature flags.
    // 0.x handed sbatch the script itself; sbatch spools its script into the
    // controller (MaxScriptSize, 4 MB by default), which a Rust binary with
    // zellij inside cannot pass through, so the job is a one-line `--wrap`
    // that execs the binary from wherever it is installed (shared FS).
    let exe = current_exe()?;
    let mut job_cmd: Vec<String> = vec![
        "exec".into(),
        shell_quote(&exe.to_string_lossy()),
        "__job".into(),
    ];
    let mouse = if args.no_mouse {
        false
    } else {
        args.mouse || ctx.cfg.mouse
    };
    if mouse {
        job_cmd.push("--mouse".to_string());
    }
    if let Some(n) = &name {
        job_cmd.push(shell_quote(&format!("--session-name={n}")));
    }
    // Carried rather than recomputed in the session: the reservation is read
    // once, at submit time, and the session should say the same thing about
    // its own allocation for its whole life even if the reservation moves.
    if let Some(m) = &maint_carry {
        job_cmd.push(shell_quote(&format!("--maint={}@{}", m.name, m.ends_epoch)));
    }
    let mut submit = sbatch_args.clone();
    submit.push(format!("--wrap={}", job_cmd.join(" ")));

    let submission = match ctx.slurm.submit(&submit) {
        Ok(s) => s,
        Err(SlurmError::Failed { stderr, status, .. }) => {
            eprintln!("{err}{bold}Error:{reset}{err} job submission failed. sbatch said:{reset}");
            eprintln!();
            for line in stderr.lines() {
                eprintln!("  {line}");
            }
            eprintln!();
            if !args.sbatch_args.is_empty() {
                eprintln!(
                    "{dim}Note: these options were not recognized by sinteractive and were{reset}"
                );
                eprintln!(
                    "{dim}passed through to sbatch: {}{reset}",
                    args.sbatch_args.join(" ")
                );
                eprintln!();
            }
            eprintln!("{dim}Run 'sinteractive --help' to see available options.{reset}");
            return Ok(if status > 0 { status } else { 1 });
        }
        Err(e) => return Err(anyhow!("{e}")),
    };
    if !submission.warnings.is_empty() {
        eprintln!("{}", submission.warnings);
    }
    let job_id = submission.job_id;

    // From here a Ctrl-C cancels the job (0.x `cleanup` trap) until the
    // session is confirmed up.
    arm();

    ctx.slurm
        .set_comment(job_id, &comment_for(name.as_deref()))?;
    eprintln!(
        "{dim}Submitted job{reset} {id}{job_id}{reset}{dim}, waiting for it to start.{reset}"
    );

    match wait_for_running(&ctx.slurm, job_id, &p)? {
        Waited::Running => {}
        Waited::Interrupted => return Ok(cancelled_by_user(&ctx.slurm, job_id, &p)),
        Waited::Gone => {
            eprintln!(
                "{err}{bold}sinteractive:{reset}{err} job is neither RUNNING nor PENDING. Aborting.{reset}"
            );
            // The job left the queue on its own (failed or completed); no
            // "Cancelled job" message, which is meant for a user interrupt.
            disarm();
            scancel_quiet(&ctx.slurm, job_id);
            return Ok(1);
        }
    }

    let node = match ctx.slurm.batch_host(job_id)? {
        Some(n) => n,
        None if interrupted() => return Ok(cancelled_by_user(&ctx.slurm, job_id, &p)),
        None => {
            disarm();
            return Err(anyhow!(
                "job {job_id} is RUNNING but squeue reports no batch host"
            ));
        }
    };
    eprintln!();
    eprintln!("{dim}Allocated {reset}{id}{node}{reset}{dim} — bringing up the session...{reset}");

    // A RUNNING job only means the allocation exists; the batch script still
    // has to bring up the session. Poll for it (a single ssh that retries on
    // the node) instead of guessing a fixed sleep.
    let connected = wait_until_ready(&node, job_id);
    if interrupted() {
        return Ok(cancelled_by_user(&ctx.slurm, job_id, &p));
    }

    // The session is up (or we gave up waiting); from this point a detach
    // or disconnect should NOT cancel the job.
    disarm();

    if !connected {
        eprintln!();
        eprintln!(
            "{err}{bold}sinteractive:{reset}{err} session did not come up on {node} within 30s.{reset}"
        );
        eprintln!("{dim}Job {job_id} may still be starting. Reconnect with:{reset}");
        eprintln!("  sinteractive attach {id}{job_id}{reset}");
        return Ok(1);
    }

    let attach_target = name.clone().unwrap_or_else(|| job_id.to_string());

    // Headless launch: report how to reach the session and return without
    // attaching (there may be no terminal to attach from). With --json the
    // status object is the whole stdout.
    if args.detach {
        if args.json {
            let Some(row) = ctx.slurm.job(job_id)? else {
                print_json(&SessionInfo::not_found(job_id))?;
                return Ok(1);
            };
            let mut info = SessionInfo::from_row(&row, now_epoch());
            info.created = created;
            print_json(&info)?;
        }
        eprintln!();
        eprintln!(
            "{ok}{bold}✓{reset} {bold}Session {reset}{id}{job_id}{reset}{bold} is ready on {reset}{id}{node}{reset}{bold}.{reset}"
        );
        eprintln!();
        eprintln!("  {key}Attach:{reset}   sinteractive attach {attach_target}");
        eprintln!("  {key}Status:{reset}   sinteractive status {attach_target}");
        eprintln!("  {key}Cancel:{reset}   sinteractive cancel {attach_target}");
        return Ok(0);
    }

    let _ = Command::new("ssh")
        .args(["-X", "-t", &node])
        .arg(&exe)
        .arg("__attach")
        .arg(format!("sinteractive-{job_id}"))
        .status();

    // Give the batch script a moment to finish shutting down.
    std::thread::sleep(fast_poll().unwrap_or(Duration::from_secs(3)));
    teardown_summary(ctx, job_id, &node, &attach_target, &p)?;
    Ok(0)
}

/// After the attach returns: is the job still there? (script lines 799-856)
fn teardown_summary(
    ctx: &Ctx,
    job_id: u64,
    node: &str,
    attach_target: &str,
    p: &Palette,
) -> Result<()> {
    let (reset, bold, dim, key, id, warn) = (&p.reset, &p.bold, &p.dim, &p.key, &p.id, &p.warn);
    let still_running = ctx.slurm.job(job_id)?.is_some_and(|r| r.state == "RUNNING");
    let running = ctx.running_sessions()?;
    if still_running {
        eprintln!();
        eprintln!(
            "{warn}{bold}⠿{reset} {bold}Detached.{reset} {dim}Job{reset} {id}{job_id}{reset} {dim}is still running on{reset} {id}{node}{reset}{dim}.{reset}"
        );
        eprintln!();
        eprintln!("  {key}Reconnect:{reset}  sinteractive attach {attach_target}");
        eprintln!("  {key}Cancel:{reset}     scancel {job_id}");
        let others: Vec<&JobRow> = running.iter().filter(|r| r.job_id != job_id).collect();
        if !others.is_empty() {
            eprintln!();
            eprintln!("{dim}Other running sinteractive sessions:{reset}");
            eprintln!("{}", session_table_line(None, p));
            for r in others {
                eprintln!("{}", session_table_line(Some(r), p));
            }
        }
    } else {
        eprintln!(
            "{dim}Session ended. Job{reset} {id}{job_id}{reset} {dim}is no longer running.{reset}"
        );
        if !running.is_empty() {
            eprintln!();
            eprintln!("{warn}You have other sinteractive sessions still running:{reset}");
            eprintln!("{}", session_table_line(None, p));
            for r in &running {
                eprintln!("{}", session_table_line(Some(r), p));
            }
            eprintln!();
            eprintln!("{dim}To free slots, cancel sessions you no longer need:{reset}");
            for r in &running {
                eprintln!("  scancel {}", r.job_id);
            }
        }
    }
    Ok(())
}

/// Print a session's human status block on stdout (for `ensure`), followed
/// by its active notices — the full text behind the status line's
/// "⚠ N notices" indicator, readable without attaching (script line 1127).
pub(crate) fn print_status_human(ctx: &Ctx, info: &SessionInfo) {
    let p = ctx.palette(1);
    print!("{}", render_status(info, &p));
    for n in notices::read(&ctx.state, info.job_id) {
        if n.text.is_empty() {
            continue;
        }
        let c = if n.is_severe() {
            format!("{}{}", p.err, p.bold)
        } else {
            p.warn.clone()
        };
        println!(
            "  {}{:<11}{} {c}{}{}",
            p.key, "Notice:", p.reset, n.text, p.reset
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn has_flag_forms() {
        assert!(has_flag(&v(&["--time=8h"]), &["--time", "-t"]));
        assert!(has_flag(&v(&["--time", "8h"]), &["--time", "-t"]));
        assert!(has_flag(&v(&["-t", "8h"]), &["--time", "-t"]));
        assert!(has_flag(&v(&["-t8h"]), &["--time", "-t"]));
        assert!(!has_flag(&v(&["--time-min=5"]), &["--time", "-t"]));
        assert!(!has_flag(&v(&["--tmp=1G"]), &["--time"]));
        // --mem is a prefix match: --mem-per-cpu also satisfies it.
        assert!(has_flag(&v(&["--mem-per-cpu=1G"]), &["--mem"]));
        assert!(has_flag(&v(&["-J", "x"]), &["--job-name", "-J"]));
    }

    #[test]
    fn flag_value_forms() {
        assert_eq!(
            flag_value(&v(&["--partition=rna"]), "--partition", Some("-p")).as_deref(),
            Some("rna")
        );
        assert_eq!(
            flag_value(&v(&["-p", "rna"]), "--partition", Some("-p")).as_deref(),
            Some("rna")
        );
        assert_eq!(
            flag_value(&v(&["--partition", "rna"]), "--partition", Some("-p")).as_deref(),
            Some("rna")
        );
        assert_eq!(flag_value(&v(&["-x"]), "--partition", Some("-p")), None);
    }

    #[test]
    fn defaults_go_first_and_respect_overrides() {
        let mut cfg = Config::defaults();
        cfg.qos = Some("cpu-normal".into());
        let mut args = v(&["--gres=gpu:1", "-t", "2h"]);
        let mut warn = Vec::new();
        apply_defaults(&cfg, &mut args, &mut warn);
        assert_eq!(
            args,
            v(&[
                "--partition=interactive",
                "--qos=cpu-normal",
                "--cpus-per-task=2",
                "--mem=8G",
                "--gres=gpu:1",
                "-t",
                "2h"
            ])
        );
        assert!(warn.is_empty());
        assert_eq!(partition_of(&args, &cfg), "interactive");
        assert_eq!(qos_of(&args, &cfg, "interactive"), "cpu-normal");
        assert_eq!(qos_of(&v(&["--qos=long"]), &cfg, "x"), "long");
        assert_eq!(qos_of(&[], &Config::defaults(), "rna"), "rna");
    }

    #[test]
    fn own_args_translate_and_normalise_time() {
        let args = LaunchArgs {
            node: Some("n01".into()),
            time: Some("8h".into()),
            partition: Some("rna".into()),
            threads: Some(4),
            mem: Some("16G".into()),
            ..LaunchArgs::default()
        };
        let mut warn = Vec::new();
        assert_eq!(
            own_sbatch_args(&args, &mut warn),
            v(&[
                "--nodelist=n01",
                "--time=08:00:00",
                "--partition=rna",
                "--cpus-per-task=4",
                "--mem=16G"
            ])
        );
        let args = LaunchArgs {
            time: Some("8x".into()),
            ..LaunchArgs::default()
        };
        assert_eq!(own_sbatch_args(&args, &mut warn), v(&["--time=8x"]));
        assert_eq!(warn.len(), 1);
    }

    #[test]
    fn pend_reasons_and_probe() {
        assert_eq!(pend_reason("Resources"), "waiting for free resources");
        assert_eq!(pend_reason("None"), "waiting for the job to start");
        assert_eq!(
            pend_reason("QOSMaxJobsPerUserLimit"),
            "waiting (QOSMaxJobsPerUserLimit)"
        );
        let probe = readiness_probe(42, "1");
        assert!(probe.contains("/sint-42/ready"), "{probe}");
        assert!(probe.contains("seq 1 30"), "{probe}");
        assert_eq!(est_start("N/A", 0), "");
        assert_eq!(est_start("2000-01-01T00:00:00", now_epoch()), "");
    }
}
