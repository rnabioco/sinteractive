//! `sinteractive queue [--all] [--watch] [--json]` — your jobs: what is
//! running, what is pending and why, and the last day's history with a
//! memory right-sizing hint. `--all` adds everyone's jobs per partition;
//! `--watch` redraws every 5 s (Ctrl-C exits).

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;
use sint_core::color::Palette;
use sint_core::now_epoch;
use sint_core::session::SessionInfo;
use sint_core::slurm::sacct::AccountedJob;
use sint_core::slurm::squeue::{gpus_from_tres, mem_to_mb, JobRow};
use sint_core::slurm::Slurm;
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
        let names = job_names(&ctx.slurm);
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
    loop {
        let snap = Snapshot::gather(&ctx, args.all)?;
        let mut out = std::io::stdout().lock();
        // Clear and home, then the frame in one write so it never flickers.
        let stamp = time::OffsetDateTime::now_local()
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
            .format(time::macros::format_description!(
                "[hour]:[minute]:[second]"
            ))
            .unwrap_or_default();
        let _ = write!(
            out,
            "\x1b[2J\x1b[H{}sinteractive queue{} {}{stamp} — every 5 s, Ctrl-C to exit{}\n\n{}",
            p.bold,
            p.reset,
            p.dim,
            p.reset,
            render(&snap, &p)
        );
        let _ = out.flush();
        drop(out);
        std::thread::sleep(WATCH_EVERY);
    }
}

fn is_finished(state: &str) -> bool {
    FINISHED.iter().any(|f| state.starts_with(f))
}

/// `squeue --me -h -o '%i|%j'`: job names, which the row contract does not
/// carry. Empty when squeue has nothing to say.
fn job_names(slurm: &Slurm) -> HashMap<u64, String> {
    let mut names = HashMap::new();
    let Ok(out) = slurm.run("squeue", &["--me", "-h", "-o", "%i|%j"]) else {
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
