//! `sinteractive queue [--all] [--watch] [--json]` — your jobs: what is
//! running, what is pending and why, and the last day's history with a
//! memory right-sizing hint. `--all` adds everyone's jobs per partition;
//! `--watch` redraws every 5 s and ends on `q`, `Esc` or Ctrl-C.
//!
//! The watch view is what `Ctrl+b q` opens in a floating pane, so it needs
//! the key that closes every other sint view: leaving it Ctrl-C-only made
//! the one popup you cannot quit the way you quit the monitor.

use std::collections::{BTreeMap, HashMap};
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use serde::Serialize;
use sint_core::color::Palette;
use sint_core::now_epoch;
use sint_core::session::SessionInfo;
use sint_core::slurm::sacct::AccountedJob;
use sint_core::slurm::squeue::{gpus_from_tres, mem_to_mb, JobRow};
use sint_core::time::{format_short_duration, slurm_timestamp_to_epoch};

use super::common::{pend_reason, print_json, Ctx};
use crate::cli::QueueArgs;

/// States shown in the "Recent" table (sacct spells them out; `CANCELLED`
/// carries a `by UID` suffix).
const FINISHED: [&str; 6] = [
    "COMPLETED",
    "FAILED",
    "CANCELLED",
    "TIMEOUT",
    "OUT_OF_MEMORY",
    "OOM",
];

const WATCH_EVERY: Duration = Duration::from_secs(5);

/// Rows the watch frame keeps for itself: the title, the key legend under
/// it, and the blank before the tables.
const WATCH_CHROME: usize = 3;

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PartitionSummary {
    pub partition: String,
    pub running: usize,
    pub pending: usize,
}

/// One job in the `queue --json` `running`/`pending` arrays: the session
/// status object plus Slurm's job name and, for a pending job, its reason
/// and estimated start (present but null when Slurm has no estimate).
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct QueueRow {
    #[serde(flatten)]
    pub info: SessionInfo,
    pub job_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub est_start_epoch: Option<Option<i64>>,
}

/// The `queue --json` object.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct QueueReport {
    pub running: Vec<QueueRow>,
    pub pending: Vec<QueueRow>,
    /// Finished in the last day, newest first; empty when sacct failed.
    pub recent: Vec<AccountedJob>,
    /// Per-partition counts over everyone's jobs (`--all` only).
    pub partitions: Vec<PartitionSummary>,
}

/// The `queue --json` object, gathered now.
pub fn queue_data(ctx: &Ctx, all: bool) -> Result<QueueReport> {
    Ok(Snapshot::gather(ctx, all)?.report())
}

/// One gather of everything the views show.
struct Snapshot {
    now: i64,
    running: Vec<JobRow>,
    pending: Vec<JobRow>,
    names: HashMap<u64, String>,
    recent: Result<Vec<AccountedJob>, String>,
    partitions: Option<Vec<PartitionSummary>>,
}

impl Snapshot {
    fn gather(ctx: &Ctx, all: bool) -> Result<Self> {
        let rows = ctx
            .slurm
            .my_jobs(&["RUNNING", "PENDING", "COMPLETING", "CONFIGURING"])?;
        let (pending, running): (Vec<JobRow>, Vec<JobRow>) =
            rows.into_iter().partition(|r| r.state == "PENDING");
        let names = ctx.slurm.my_job_names(&[]);
        let recent = ctx
            .slurm
            .recent_jobs("now-1day")
            .map(|jobs| {
                let mut jobs: Vec<AccountedJob> =
                    jobs.into_iter().filter(|j| is_finished(&j.state)).collect();
                jobs.sort_by_key(|j| std::cmp::Reverse(j.end_epoch));
                jobs
            })
            .map_err(|e| e.to_string());
        let partitions = if all {
            Some(partition_summary(
                &ctx.slurm.run("squeue", &["-h", "-o", "%P|%T"])?,
            ))
        } else {
            None
        };
        Ok(Snapshot {
            now: now_epoch(),
            running,
            pending,
            names,
            recent,
            partitions,
        })
    }
}

pub fn run(args: QueueArgs) -> Result<i32> {
    let ctx = Ctx::new();
    if args.json {
        print_json(&queue_data(&ctx, args.all)?)?;
        return Ok(0);
    }
    let p = ctx.palette(1);
    if !args.watch {
        let snap = Snapshot::gather(&ctx, args.all)?;
        print!("{}", render(&snap, &p));
        return Ok(0);
    }
    // Keys need raw mode — cooked stdin would hold `q` until Enter. Only
    // with a terminal on both ends: piped or redirected output stays the
    // plain redraw loop it was, ended by a signal.
    let keys = std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && enable_raw_mode().is_ok();
    let _raw = RawGuard(keys);
    loop {
        let snap = Snapshot::gather(&ctx, args.all)?;
        let stamp = time::OffsetDateTime::now_local()
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
            .format(time::macros::format_description!(
                "[hour]:[minute]:[second]"
            ))
            .unwrap_or_default();
        let size = terminal_size()
            .ok()
            .map(|(cols, rows)| (cols as usize, rows as usize));
        let frame = watch_frame(&render(&snap, &p), &p, &stamp, size, keys);
        // Raw mode drops the ONLCR translation, so the frame carries its own
        // carriage returns or every row steps one column right.
        let frame = if keys {
            frame.replace('\n', "\r\n")
        } else {
            frame
        };
        let mut out = std::io::stdout().lock();
        // Clear and home, then the frame in one write so it never flickers.
        let _ = write!(out, "\x1b[2J\x1b[H{frame}");
        let _ = out.flush();
        drop(out);
        if !keys {
            std::thread::sleep(WATCH_EVERY);
            continue;
        }
        if let Wait::Quit = wait(WATCH_EVERY)? {
            // Leave the cursor on a line of its own: raw mode left it
            // wherever the frame ended, and a shell prompt would land there.
            println!("\r");
            return Ok(0);
        }
    }
}

/// One redraw: a title saying what this is, the key legend right under it
/// — the way out is the first thing to read, not something to hunt for at
/// the foot of a pane — then the tables.
///
/// The body is clipped to `size` rather than allowed to overflow. The
/// floating pane `Ctrl+b q` opens is short and the recent list is long, and
/// a frame taller than the pane scrolls its own title and legend off the
/// top — which is what left the popup looking like a wall of rows with no
/// name on it and no visible way out.
fn watch_frame(
    body: &str,
    p: &Palette,
    stamp: &str,
    size: Option<(usize, usize)>,
    keys: bool,
) -> String {
    let (reset, bold, dim) = (&p.reset, &p.bold, &p.dim);
    let cols = size.map(|(c, _)| c).unwrap_or(usize::MAX);
    // Widest first: the subtitle goes when the pane is too narrow for it,
    // then the clock. A title that wraps costs a body row and pushes the
    // legend down.
    let (name, sub, clock) = (
        "sinteractive queue",
        " — running, pending, recent",
        format!("  {stamp}"),
    );
    let w = |s: &str| s.chars().count();
    let title = if w(name) + w(sub) + w(&clock) <= cols {
        format!("{bold}{name}{reset}{dim}{sub}{clock}{reset}")
    } else if w(name) + w(&clock) <= cols {
        format!("{bold}{name}{reset}{dim}{clock}{reset}")
    } else {
        format!("{bold}{name}{reset}")
    };
    let legend = watch_legend(p, keys, cols);

    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let Some((_, rows)) = size else {
        return format!("{title}\n{legend}\n\n{}\n", lines.join("\n"));
    };
    let room = rows.saturating_sub(WATCH_CHROME);
    if room == 0 {
        return format!("{title}\n{legend}");
    }
    if lines.len() > room {
        // The clipped rows are the oldest of the recent history, so the
        // count doubles as the pointer to the command that prints them all.
        let hidden = lines.len() - room + 1;
        lines.truncate(room - 1);
        lines.push(format!(
            "  {dim}… {hidden} more — `sinteractive queue` prints them all{reset}"
        ));
    }
    format!("{title}\n{legend}\n\n{}", lines.join("\n"))
}

/// The legend, dropped to its shortest form on a narrow pane. The way out
/// comes first and is never dropped: it is the one thing someone who opened
/// the popup by accident needs, so on a pane too narrow even for that it
/// is still printed and the terminal clips it.
fn watch_legend(p: &Palette, keys: bool, cols: usize) -> String {
    let (reset, dim, key) = (&p.reset, &p.dim, &p.key);
    if !keys {
        return format!("{dim}redraws every 5 s — Ctrl-C to exit{reset}");
    }
    // Each part carries its own plain text, because the styled one is mostly
    // escapes and `len()` on it means nothing.
    let parts = [
        (
            "q or Esc closes this",
            format!("{key}q{reset}{dim} or {reset}{key}Esc{reset}{dim} closes this{reset}"),
        ),
        (
            "   r refreshes",
            format!("   {key}r{reset}{dim} refreshes{reset}"),
        ),
        (
            "   redraws every 5 s",
            format!("   {dim}redraws every 5 s{reset}"),
        ),
    ];
    let mut line = String::new();
    let mut used = 0;
    for (i, (plain, styled)) in parts.into_iter().enumerate() {
        used += plain.chars().count();
        if used > cols && i > 0 {
            break;
        }
        line.push_str(&styled);
    }
    line
}

/// Puts the terminal back into cooked mode on every exit path — a `?` out
/// of the watch loop included, which is why it is a guard and not a call at
/// the end.
struct RawGuard(bool);

impl Drop for RawGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = disable_raw_mode();
        }
    }
}

/// What ended the pause between redraws.
enum Wait {
    Quit,
    Redraw,
}

/// Wait up to `budget` for a keypress.
///
/// `q` and `Esc` are the monitor TUI's keys; Ctrl-C and Ctrl-D are here
/// because raw mode delivers them as ordinary keys rather than as a signal
/// or EOF, and the habit of pressing them must keep working. `r` and a
/// resize redraw at once instead of finishing the five seconds.
fn wait(budget: Duration) -> Result<Wait> {
    let deadline = Instant::now() + budget;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || !event::poll(left)? {
            return Ok(Wait::Redraw);
        }
        match event::read()? {
            Event::Resize(..) => return Ok(Wait::Redraw),
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('q' | 'Q') | KeyCode::Esc => return Ok(Wait::Quit),
                KeyCode::Char('c' | 'd') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(Wait::Quit)
                }
                KeyCode::Char('r' | 'R') => return Ok(Wait::Redraw),
                _ => {}
            },
            _ => {}
        }
    }
}

fn is_finished(state: &str) -> bool {
    FINISHED.iter().any(|f| state.starts_with(f))
}

/// `squeue -h -o '%P|%T'` over everyone → per-partition running/pending.
fn partition_summary(output: &str) -> Vec<PartitionSummary> {
    let mut map: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for line in output.lines() {
        let Some((part, state)) = line.split_once('|') else {
            continue;
        };
        let e = map.entry(part.trim().to_string()).or_default();
        match state.trim() {
            "RUNNING" => e.0 += 1,
            "PENDING" => e.1 += 1,
            _ => {}
        }
    }
    map.into_iter()
        .map(|(partition, (running, pending))| PartitionSummary {
            partition,
            running,
            pending,
        })
        .collect()
}

/// `in 2h 5m` when Slurm's estimate is ahead of now, else empty.
fn est_start(start_time: &str, now: i64) -> String {
    match slurm_timestamp_to_epoch(start_time) {
        Some(start) if start > now => format!("in {}", format_short_duration(start - now)),
        _ => String::new(),
    }
}

/// A `--mem` worth asking for next time: MaxRSS with 50% headroom, rounded
/// up to 256M steps below a gigabyte and whole gigabytes above.
fn suggest_mem(max_rss_mb: u64) -> String {
    let headroom = (max_rss_mb * 3).div_ceil(2).max(1);
    if headroom < 1024 {
        format!("{}M", headroom.div_ceil(256) * 256)
    } else {
        format!("{}G", headroom.div_ceil(1024))
    }
}

/// `↓ could use N` when the job used under half of what it asked for.
fn mem_hint(req_mem: &str, max_rss: &str) -> Option<String> {
    let req = mem_to_mb(req_mem)?;
    let used = mem_to_mb(max_rss)?;
    if req == 0 || used * 2 >= req {
        return None;
    }
    Some(format!("↓ could use {}", suggest_mem(used)))
}

fn fmt_mb(mb: u64) -> String {
    if mb >= 1024 {
        let g = mb as f64 / 1024.0;
        if g.fract() == 0.0 {
            format!("{g:.0}G")
        } else {
            format!("{g:.1}G")
        }
    } else {
        format!("{mb}M")
    }
}

/// `used of requested` from sacct's raw strings, in one unit family.
fn mem_column(req_mem: &str, max_rss: &str) -> String {
    let req = mem_to_mb(req_mem)
        .map(fmt_mb)
        .unwrap_or_else(|| req_mem.to_string());
    match mem_to_mb(max_rss) {
        Some(used) => format!("{} of {req}", fmt_mb(used)),
        None if max_rss.is_empty() => format!("? of {req}"),
        None => format!("{max_rss} of {req}"),
    }
}

fn state_colour<'a>(state: &str, p: &'a Palette) -> &'a str {
    if state.starts_with("COMPLETED") {
        &p.ok
    } else if state.starts_with("CANCELLED") {
        &p.warn
    } else {
        &p.err
    }
}

fn render(snap: &Snapshot, p: &Palette) -> String {
    let (reset, bold, dim, id) = (&p.reset, &p.bold, &p.dim, &p.id);
    let name_of = |job_id: u64| snap.names.get(&job_id).cloned().unwrap_or_default();
    let mut out = String::new();

    out.push_str(&format!("{bold}Running{reset} ({})\n", snap.running.len()));
    if snap.running.is_empty() {
        out.push_str(&format!("  {dim}no running jobs{reset}\n"));
    } else {
        out.push_str(&format!(
            "  {dim}{:<10}  {:<20}  {:<12}  {:<16}  {:<20}  {:>4}  {:>4}{reset}\n",
            "JOBID", "NAME", "PARTITION", "NODE", "ELAPSED/LIMIT", "CPUS", "GPUS"
        ));
        for r in &snap.running {
            let node = if r.state == "RUNNING" {
                r.node.clone()
            } else {
                format!("{} ({})", r.node, r.state)
            };
            out.push_str(&format!(
                "  {id}{:<10}{reset}  {:<20}  {:<12}  {:<16}  {:<20}  {:>4}  {:>4}\n",
                r.job_id,
                truncate(&name_of(r.job_id), 20),
                truncate(&r.partition, 12),
                truncate(&node, 16),
                format!("{}/{}", r.elapsed, r.time_limit),
                r.cpus.map(|c| c.to_string()).unwrap_or_default(),
                gpus_from_tres(&r.tres_per_node),
            ));
        }
    }

    out.push_str(&format!(
        "\n{bold}Pending{reset} ({})\n",
        snap.pending.len()
    ));
    if snap.pending.is_empty() {
        out.push_str(&format!("  {dim}no pending jobs{reset}\n"));
    } else {
        out.push_str(&format!(
            "  {dim}{:<10}  {:<20}  {:<12}  {:<36}  {}{reset}\n",
            "JOBID", "NAME", "PARTITION", "REASON", "EST. START"
        ));
        for r in &snap.pending {
            out.push_str(&format!(
                "  {id}{:<10}{reset}  {:<20}  {:<12}  {:<36}  {}\n",
                r.job_id,
                truncate(&name_of(r.job_id), 20),
                truncate(&r.partition, 12),
                truncate(&pend_reason(&r.reason), 36),
                est_start(&r.start_time, snap.now),
            ));
        }
    }

    out.push_str(&format!("\n{bold}Recent{reset} (last 24 h)\n"));
    match &snap.recent {
        Err(e) => out.push_str(&format!("  {dim}history unavailable: {e}{reset}\n")),
        Ok(jobs) if jobs.is_empty() => {
            out.push_str(&format!("  {dim}no finished jobs{reset}\n"));
        }
        Ok(jobs) => {
            out.push_str(&format!(
                "  {dim}{:<10}  {:<20}  {:<12}  {:<11}  {}{reset}\n",
                "JOBID", "NAME", "STATE", "ELAPSED", "MEMORY (used of requested)"
            ));
            for j in jobs {
                let state = j.state.split_whitespace().next().unwrap_or("");
                let hint = mem_hint(&j.req_mem, &j.max_rss)
                    .map(|h| format!("  {}{h}{reset}", p.key))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {id}{:<10}{reset}  {:<20}  {}{:<12}{reset}  {:<11}  {}{hint}\n",
                    j.job_id,
                    truncate(&j.name, 20),
                    state_colour(state, p),
                    truncate(state, 12),
                    j.elapsed,
                    mem_column(&j.req_mem, &j.max_rss),
                ));
            }
        }
    }

    if let Some(parts) = &snap.partitions {
        out.push_str(&format!("\n{bold}Partitions{reset} (everyone's jobs)\n"));
        if parts.is_empty() {
            out.push_str(&format!("  {dim}queue is empty{reset}\n"));
        } else {
            out.push_str(&format!(
                "  {dim}{:<16}  {:>8}  {:>8}{reset}\n",
                "PARTITION", "RUNNING", "PENDING"
            ));
            for s in parts {
                out.push_str(&format!(
                    "  {:<16}  {:>8}  {:>8}\n",
                    truncate(&s.partition, 16),
                    s.running,
                    s.pending
                ));
            }
        }
    }
    out
}

/// Cut to `width` characters with an ellipsis, so a long name cannot push
/// the columns to its right.
fn truncate(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n <= width {
        return s.to_string();
    }
    let mut t: String = s.chars().take(width.saturating_sub(1)).collect();
    t.push('…');
    t
}

impl Snapshot {
    fn report(&self) -> QueueReport {
        let row = |r: &JobRow| QueueRow {
            info: SessionInfo::from_row(r, self.now),
            job_name: self.names.get(&r.job_id).cloned(),
            reason: (r.state == "PENDING").then(|| r.reason.clone()),
            est_start_epoch: (r.state == "PENDING")
                .then(|| slurm_timestamp_to_epoch(&r.start_time)),
        };
        QueueReport {
            running: self.running.iter().map(row).collect(),
            pending: self.pending.iter().map(row).collect(),
            recent: self.recent.clone().unwrap_or_default(),
            partitions: self.partitions.clone().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watch_frame_names_itself_and_says_how_to_leave_up_top() {
        let p = Palette::none();
        let body: String = (0..40).map(|i| format!("row {i}\n")).collect();
        let frame = watch_frame(&body, &p, "12:00:00", Some((80, 12)), true);
        let lines: Vec<&str> = frame.lines().collect();
        // Exactly the pane: title, legend, blank, 9 body rows. Any more and
        // the terminal scrolls the title and the legend out of sight.
        assert_eq!(lines.len(), 12, "{frame}");
        assert!(
            lines[0].starts_with("sinteractive queue — running, pending"),
            "{frame}"
        );
        assert!(lines[0].ends_with("12:00:00"), "{frame}");
        assert_eq!(
            lines[1],
            "q or Esc closes this   r refreshes   redraws every 5 s"
        );
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "row 0");
        assert!(lines[11].contains("… 32 more"), "{frame}");

        // A body that fits: same chrome, then the rows, nothing padded.
        let short = watch_frame("row 0\nrow 1\n", &p, "12:00:00", Some((80, 12)), true);
        let lines: Vec<&str> = short.lines().collect();
        assert_eq!(lines.len(), 5, "{short}");
        assert!(lines[1].starts_with("q or Esc closes this"), "{short}");
        assert_eq!(lines[3], "row 0");

        // Narrow: the title loses its subtitle and the legend its extras;
        // the way out survives both, even on a pane too narrow to hold it.
        let narrow = watch_frame(&body, &p, "12:00:00", Some((22, 12)), true);
        let lines: Vec<&str> = narrow.lines().collect();
        assert_eq!(lines[0], "sinteractive queue");
        assert_eq!(lines[1], "q or Esc closes this");
        let tiny = watch_frame(&body, &p, "12:00:00", Some((8, 12)), true);
        assert_eq!(tiny.lines().nth(1), Some("q or Esc closes this"));

        // Without keys the legend says what does work instead.
        let piped = watch_frame(&body, &p, "12:00:00", None, false);
        assert!(piped.contains("Ctrl-C to exit"), "{piped}");
        assert!(piped.contains("row 39"), "no clipping without a size");
    }

    #[test]
    fn memory_hints() {
        assert_eq!(suggest_mem(1), "256M");
        assert_eq!(suggest_mem(300), "512M");
        assert_eq!(suggest_mem(700), "2G");
        assert_eq!(suggest_mem(4096), "6G");
        assert_eq!(
            mem_hint("32G", "1234K").as_deref(),
            Some("↓ could use 256M")
        );
        assert_eq!(mem_hint("64G", "40G"), None);
        assert_eq!(mem_hint("64G", ""), None);
        assert_eq!(mem_hint("N/A", "1G"), None);
        assert_eq!(mem_column("4000M", "1234K"), "1M of 3.9G");
        assert_eq!(mem_column("32G", ""), "? of 32G");
    }

    #[test]
    fn partitions_and_states() {
        let parts = partition_summary(
            "rna|RUNNING\namilan|PENDING\nrna|PENDING\nrna|RUNNING\nx|COMPLETING\n",
        );
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].partition, "amilan");
        assert_eq!((parts[0].running, parts[0].pending), (0, 1));
        assert_eq!((parts[1].running, parts[1].pending), (2, 1));
        assert!(is_finished("CANCELLED by 12345"));
        assert!(is_finished("OUT_OF_MEMORY"));
        assert!(!is_finished("RUNNING"));
        assert_eq!(est_start("N/A", 0), "");
        assert_eq!(est_start("2000-01-01T00:00:00", now_epoch()), "");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }
}
