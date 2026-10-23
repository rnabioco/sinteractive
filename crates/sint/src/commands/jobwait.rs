//! Waiting on Slurm jobs and reading their logs: the data behind the MCP
//! tools `wait_for_jobs` and `job_log`.
//!
//! Neither is limited to the sampler's jobs. A job counts as running while
//! `squeue` still lists it in a live state; once it has left (or `squeue`
//! shows it ended), one `sacct` call says how it went. The log is read from
//! the path Slurm recorded for stdout — `scontrol` while the job is live,
//! `sacct` after — on the shared filesystem.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;
use sint_core::slurm::sacct::JobOutcome;
use sint_core::slurm::Slurm;

/// How often `squeue` is asked again while waiting.
pub const POLL: Duration = Duration::from_secs(5);
/// How far back a finished job is looked up by name.
const NAME_LOOKBACK: &str = "now-1day";
/// A log is read from its last this many bytes.
const LOG_TAIL_BYTES: u64 = 256 * 1024;

/// `wait_for_jobs`' answer.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct WaitedJobs {
    pub jobs: Vec<JobOutcome>,
    /// Some job was still unfinished when the time ran out; `jobs` then
    /// carries where each stands.
    pub timed_out: bool,
}

/// `job_log`'s answer.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct JobLog {
    pub path: String,
    /// The last lines of the file; empty when it does not exist (yet).
    pub lines: Vec<String>,
    pub exists: bool,
}

/// Whether `state` (squeue's or sacct's) is one a job does not leave.
pub fn is_terminal(state: &str) -> bool {
    matches!(
        state.split_whitespace().next().unwrap_or(""),
        "COMPLETED"
            | "FAILED"
            | "CANCELLED"
            | "TIMEOUT"
            | "OUT_OF_MEMORY"
            | "NODE_FAIL"
            | "PREEMPTED"
            | "BOOT_FAIL"
            | "DEADLINE"
    )
}

/// Whether `squeue` still has `id` in a live state. A squeue that fails is
/// "yes": a hiccup must not read as every job having finished.
fn still_live(slurm: &Slurm, id: u64) -> bool {
    match slurm.job(id) {
        Ok(Some(row)) => !is_terminal(&row.state),
        Ok(None) => false,
        Err(_) => true,
    }
}

/// An outcome for a job accounting does not know.
fn unknown(job_id: u64, state: &str) -> JobOutcome {
    JobOutcome {
        job_id,
        name: String::new(),
        state: state.to_string(),
        exit_code: None,
        elapsed: String::new(),
        time_limit: String::new(),
        max_rss_mb: None,
        req_mem_mb: None,
        node: String::new(),
        stdout: None,
    }
}

/// Block until every job in `job_ids`, and every live job of the user's
/// named `name`, has left the queue or `timeout` passes, then report each.
/// With `name` alone and nothing live under it, the last day's finished jobs
/// of that name are reported straight away.
pub fn wait_for_jobs(
    slurm: &Slurm,
    job_ids: &[u64],
    name: Option<&str>,
    timeout: Duration,
    poll: Duration,
) -> Result<WaitedJobs> {
    let mut ids: Vec<u64> = job_ids.to_vec();
    let mut finished_by_name = Vec::new();
    if let Some(name) = name {
        let live = slurm
            .my_job_names(&[])
            .into_iter()
            .filter(|(_, n)| n == name)
            .map(|(id, _)| id);
        let before = ids.len();
        ids.extend(live);
        if ids.len() == before && before == 0 {
            finished_by_name = slurm.outcomes_named(name, NAME_LOOKBACK)?;
            finished_by_name.retain(|j| j.name == name);
            if finished_by_name.is_empty() {
                return Err(anyhow!(
                    "no job named '{name}' in the queue or the last day"
                ));
            }
        }
    }
    if ids.is_empty() && finished_by_name.is_empty() {
        return Err(anyhow!("give job_ids or name"));
    }
    ids.sort_unstable();
    ids.dedup();

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let mut pending = ids.clone();
    loop {
        pending.retain(|id| still_live(slurm, *id));
        if pending.is_empty() {
            break;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            timed_out = true;
            break;
        }
        std::thread::sleep(poll.min(left));
    }

    let mut jobs = finished_by_name;
    jobs.extend(outcomes_of(slurm, &ids)?);
    jobs.sort_by_key(|j| j.job_id);
    Ok(WaitedJobs { jobs, timed_out })
}

/// Outcomes for `ids`, one `sacct`; ids accounting has never heard of are
/// reported with squeue's state, or `UNKNOWN`.
fn outcomes_of(slurm: &Slurm, ids: &[u64]) -> Result<Vec<JobOutcome>> {
    let mut found = slurm.job_outcomes(ids)?;
    found.retain(|j| ids.contains(&j.job_id));
    for id in ids {
        if !found.iter().any(|j| j.job_id == *id) {
            let state = match slurm.job(*id) {
                Ok(Some(row)) => row.state,
                _ => "UNKNOWN".to_string(),
            };
            found.push(unknown(*id, &state));
        }
    }
    Ok(found)
}

/// `%j` / `%A` (job id) and `%x` (name) in a filename pattern, which sacct
/// reports unexpanded.
fn expand_pattern(path: &str, job_id: u64, name: &str) -> String {
    path.replace("%j", &job_id.to_string())
        .replace("%A", &job_id.to_string())
        .replace("%x", name)
}

/// The last `lines` lines of job `job_id`'s stdout file.
pub fn job_log(slurm: &Slurm, job_id: u64, lines: usize) -> Result<JobLog> {
    let outcome = slurm
        .job_outcomes(&[job_id])
        .ok()
        .and_then(|o| o.into_iter().find(|j| j.job_id == job_id));
    let path = slurm
        .job_stdout(job_id)
        .or_else(|| outcome.as_ref().and_then(|o| o.stdout.clone()))
        .map(|p| expand_pattern(&p, job_id, outcome.as_ref().map_or("", |o| &o.name)))
        .ok_or_else(|| anyhow!("no stdout path recorded for job {job_id}"))?;
    let tail = read_tail(&path, lines).map_err(|e| anyhow!("could not read {path}: {e}"))?;
    Ok(match tail {
        Some(lines) => JobLog {
            path,
            lines,
            exists: true,
        },
        None => JobLog {
            path,
            lines: Vec::new(),
            exists: false,
        },
    })
}

/// The last `n` lines of the file at `path`, `None` when it does not exist.
/// Only the last [`LOG_TAIL_BYTES`] are read.
fn read_tail(path: &str, n: usize) -> std::io::Result<Option<Vec<String>>> {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let len = f.metadata()?.len();
    let start = len.saturating_sub(LOG_TAIL_BYTES);
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.take(LOG_TAIL_BYTES).read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        // The window began mid-line.
        lines.remove(0);
    }
    let from = lines.len().saturating_sub(n);
    Ok(Some(
        lines[from..]
            .iter()
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states() {
        for s in [
            "COMPLETED",
            "FAILED",
            "CANCELLED by 12",
            "TIMEOUT",
            "OUT_OF_MEMORY",
        ] {
            assert!(is_terminal(s), "{s}");
        }
        for s in ["RUNNING", "PENDING", "COMPLETING", "SUSPENDED", ""] {
            assert!(!is_terminal(s), "{s}");
        }
    }

    #[test]
    fn patterns_expand() {
        assert_eq!(expand_pattern("/h/%x-%j.out", 7, "align"), "/h/align-7.out");
        assert_eq!(expand_pattern("/h/slurm-%A.out", 7, ""), "/h/slurm-7.out");
    }

    #[test]
    fn tail_reads_the_last_lines_and_notes_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("log");
        std::fs::write(&p, "a\nb\r\nc\nd\n").unwrap();
        let path = p.to_str().unwrap();
        assert_eq!(read_tail(path, 2).unwrap().unwrap(), ["c", "d"]);
        assert_eq!(read_tail(path, 50).unwrap().unwrap(), ["a", "b", "c", "d"]);
        assert_eq!(read_tail(path, 0).unwrap().unwrap(), Vec::<String>::new());
        assert!(read_tail(dir.path().join("nope").to_str().unwrap(), 5)
            .unwrap()
            .is_none());

        // Bigger than the window: the partial first line is dropped.
        let big: String = (0..100_000).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&p, &big).unwrap();
        let tail = read_tail(path, 3).unwrap().unwrap();
        assert_eq!(tail, ["line 99997", "line 99998", "line 99999"]);
    }
}
