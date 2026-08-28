//! `squeue` queries and parsers.
//!
//! Ports the calls at script lines 691-748 (pending wait), 750 (batchhost),
//! 921-1000 (`--list`), 1012-1140 (`--status`), 1140-1236 (agent context),
//! 2152 (`resolve_session_jobid`), 2431 (`refresh_end_epoch`).

use std::sync::OnceLock;

use regex::Regex;

use super::{Slurm, SlurmError};

/// One row of `squeue --me -o '%i|%k|%N|%P|%M|%l|%e|%C|%m|%b|%T|%r|%S'`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobRow {
    pub job_id: u64,
    pub comment: String,
    pub node: String,
    pub partition: String,
    pub elapsed: String,
    pub time_limit: String,
    /// Raw `%e` (`N/A`/`Unknown` when no scheduled end).
    pub end_time: String,
    pub cpus: Option<u32>,
    /// Raw `%m` (`32G`, `4000M`, `N/A`).
    pub min_memory: String,
    /// Raw `%b` TRES-per-node (`gres:gpu:2`, `gres:gpu:a100:2`, `N/A`).
    pub tres_per_node: String,
    pub state: String,
    pub reason: String,
    /// Raw `%S` estimated start.
    pub start_time: String,
}

/// The `-o` format string that produces [`JobRow`]. Keep in one place.
pub const JOB_ROW_FORMAT: &str = "%i|%k|%N|%P|%M|%l|%e|%C|%m|%b|%T|%r|%S";

/// Number of `|`-separated fields in [`JOB_ROW_FORMAT`].
const JOB_ROW_FIELDS: usize = 13;

/// Parse the pipe-delimited rows. Blank lines are skipped; a row with too
/// few fields is an error naming the line.
///
/// The comment (`%k`) is the only free-text field, so a row with *extra*
/// fields is read as a comment containing `|`: the surplus is folded back
/// into the comment rather than shifting every column after it. Such a job
/// is never an sinteractive session, but it may sit in the same queue.
pub fn parse_job_rows(output: &str) -> Result<Vec<JobRow>, SlurmError> {
    let mut rows = Vec::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        rows.push(parse_job_row(line)?);
    }
    Ok(rows)
}

fn parse_job_row(line: &str) -> Result<JobRow, SlurmError> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() < JOB_ROW_FIELDS {
        return Err(SlurmError::Parse {
            cmd: "squeue".into(),
            reason: format!(
                "expected {JOB_ROW_FIELDS} fields, got {}: {line:?}",
                fields.len()
            ),
        });
    }
    let extra = fields.len() - JOB_ROW_FIELDS;
    let comment = fields[1..=1 + extra].join("|");
    let f = |i: usize| fields[i + extra].trim().to_string();

    let job_id = parse_job_id_field(fields[0]).ok_or_else(|| SlurmError::Parse {
        cmd: "squeue".into(),
        reason: format!("bad job id {:?} in {line:?}", fields[0]),
    })?;

    Ok(JobRow {
        job_id,
        comment,
        node: f(2),
        partition: f(3),
        elapsed: f(4),
        time_limit: f(5),
        end_time: f(6),
        cpus: f(7).parse().ok(),
        min_memory: f(8),
        tres_per_node: f(9),
        state: f(10),
        reason: f(11),
        start_time: f(12),
    })
}

/// `%i` is a plain integer for the jobs this tool submits; array tasks
/// (`123_4`) and het components (`123+1`) keep the leading job id.
fn parse_job_id_field(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = &s[digits..];
    if !(rest.is_empty() || rest.starts_with('_') || rest.starts_with('+')) {
        return None;
    }
    s[..digits].parse().ok()
}

/// `gres:gpu:2` / `gres:gpu:a100:2` / `gpu:2` → 2; `N/A`/empty → 0. Always an
/// integer: "no GPUs is a fact, not a gap".
///
/// A field that mentions `gpu` without a count (`gres:gpu`) reads as 1, as
/// in the bash. Newer Slurm spellings (`gres/gpu:2`, `gres/gpu:a100=2`, a
/// `(S:0-1)` socket suffix, a comma-separated list with other GRES) all
/// resolve to the count that follows the `gpu[:type]` token.
pub fn gpus_from_tres(tres: &str) -> u32 {
    if !tres.contains("gpu") {
        return 0;
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"gpu(?::[A-Za-z0-9_.-]+?)?[:=]([0-9]+)").unwrap());
    match re.captures(tres) {
        Some(c) => c[1].parse().unwrap_or(1),
        None => 1,
    }
}

/// `32G` → 32768, `4000M` → 4000, `1T` → 1048576; `N/A`/unparseable → None.
///
/// A bare number is already megabytes (what `--mem` means to sbatch). The
/// optional `B` and per-node/per-cpu `n`/`c` suffixes squeue can add are
/// accepted and ignored.
pub fn mem_to_mb(mem: &str) -> Option<u64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^([0-9]+)([KMGT]?)B?[nc]?$").unwrap());
    let c = re.captures(mem.trim())?;
    let num: u64 = c[1].parse().ok()?;
    match &c[2] {
        "K" => Some(num / 1024),
        "" | "M" => Some(num),
        "G" => num.checked_mul(1024),
        "T" => num.checked_mul(1024 * 1024),
        _ => None,
    }
}

/// squeue exits non-zero with this on stderr for a job id it has never
/// heard of (or has long forgotten); for our purposes that is "not listed",
/// not a failure.
fn is_invalid_job_id(err: &SlurmError) -> bool {
    matches!(err, SlurmError::Failed { stderr, .. } if stderr.contains("Invalid job id"))
}

impl Slurm {
    /// All of the user's jobs in `states` (e.g. `["RUNNING"]`), as rows.
    pub fn my_jobs(&self, states: &[&str]) -> Result<Vec<JobRow>, SlurmError> {
        let states = states.join(",");
        let mut args = vec!["--me"];
        if !states.is_empty() {
            args.extend(["--states", states.as_str()]);
        }
        args.extend(["--noheader", "-o", JOB_ROW_FORMAT]);
        let out = self.run("squeue", &args)?;
        parse_job_rows(&out)
    }

    /// One job by id (any state). `Ok(None)` when squeue no longer lists it.
    pub fn job(&self, job_id: u64) -> Result<Option<JobRow>, SlurmError> {
        let id = job_id.to_string();
        let out = match self.run(
            "squeue",
            &["--jobs", &id, "--noheader", "-o", JOB_ROW_FORMAT],
        ) {
            Ok(out) => out,
            Err(e) if is_invalid_job_id(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(parse_job_rows(&out)?.into_iter().next())
    }

    /// `squeue --jobs ID --Format batchhost` → node name.
    pub fn batch_host(&self, job_id: u64) -> Result<Option<String>, SlurmError> {
        let id = job_id.to_string();
        let out = match self.run(
            "squeue",
            &["--jobs", &id, "--noheader", "--Format", "batchhost"],
        ) {
            Ok(out) => out,
            Err(e) if is_invalid_job_id(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        let host = out.lines().next().unwrap_or("").trim();
        Ok(match host {
            "" | "n/a" | "N/A" | "(null)" => None,
            h => Some(h.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
147845|sinteractive:mywork|compute20|rna|0:43|8:00:00|2026-07-04T12:02:32|8|32G|N/A|RUNNING|None|2026-07-04T04:02:32
147846|sinteractive|compute21|interactive|1:02:11|1-00:00:00|2026-07-05T03:00:21|2|4000M|gres:gpu:a100:2|RUNNING|None|2026-07-04T03:00:21

147850|nextflow run main.nf|(Priority)|rna|0:00|4:00:00|N/A|16|64G|N/A|PENDING|Priority|N/A
147851|a comment with spaces and | a pipe|compute22|rna|5:00|2:00:00|2026-07-04T06:00:00|1|N/A|N/A|RUNNING|None|2026-07-04T04:00:00
147852||compute23|rna|5:00|2:00:00|Unknown|1|512|gres/gpu:2(S:0-1)|COMPLETING|None|2026-07-04T04:00:00
";

    #[test]
    fn parses_multi_row_fixture() {
        let rows = parse_job_rows(FIXTURE).unwrap();
        assert_eq!(rows.len(), 5);

        let r = &rows[0];
        assert_eq!(r.job_id, 147845);
        assert_eq!(r.comment, "sinteractive:mywork");
        assert_eq!(r.node, "compute20");
        assert_eq!(r.partition, "rna");
        assert_eq!(r.elapsed, "0:43");
        assert_eq!(r.time_limit, "8:00:00");
        assert_eq!(r.end_time, "2026-07-04T12:02:32");
        assert_eq!(r.cpus, Some(8));
        assert_eq!(r.min_memory, "32G");
        assert_eq!(r.tres_per_node, "N/A");
        assert_eq!(r.state, "RUNNING");
        assert_eq!(r.reason, "None");
        assert_eq!(r.start_time, "2026-07-04T04:02:32");

        let r = &rows[1];
        assert_eq!(r.comment, "sinteractive");
        assert_eq!(r.tres_per_node, "gres:gpu:a100:2");
        assert_eq!(r.time_limit, "1-00:00:00");

        // Pending row: no node, N/A end and start.
        let r = &rows[2];
        assert_eq!(r.job_id, 147850);
        assert_eq!(r.comment, "nextflow run main.nf");
        assert_eq!(r.node, "(Priority)");
        assert_eq!(r.end_time, "N/A");
        assert_eq!(r.state, "PENDING");
        assert_eq!(r.reason, "Priority");
        assert_eq!(r.start_time, "N/A");

        // A comment containing the delimiter folds back into the comment.
        let r = &rows[3];
        assert_eq!(r.comment, "a comment with spaces and | a pipe");
        assert_eq!(r.node, "compute22");
        assert_eq!(r.cpus, Some(1));
        assert_eq!(r.state, "RUNNING");

        // Empty comment, bare-number memory, Unknown end.
        let r = &rows[4];
        assert_eq!(r.comment, "");
        assert_eq!(r.min_memory, "512");
        assert_eq!(r.end_time, "Unknown");
        assert_eq!(r.state, "COMPLETING");
    }

    #[test]
    fn empty_output_is_no_rows() {
        assert!(parse_job_rows("").unwrap().is_empty());
        assert!(parse_job_rows("\n\n  \n").unwrap().is_empty());
    }

    #[test]
    fn short_row_is_an_error_naming_the_line() {
        let err = parse_job_rows("147845|sinteractive|compute20\n").unwrap_err();
        match err {
            SlurmError::Parse { cmd, reason } => {
                assert_eq!(cmd, "squeue");
                assert!(reason.contains("147845|sinteractive|compute20"), "{reason}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn bad_job_id_is_an_error() {
        let line = "JOBID|c|n|p|e|l|end|1|m|t|S|r|s";
        assert!(matches!(
            parse_job_rows(line),
            Err(SlurmError::Parse { .. })
        ));
    }

    #[test]
    fn array_and_het_job_ids_keep_the_base() {
        assert_eq!(parse_job_id_field("123_4"), Some(123));
        assert_eq!(parse_job_id_field("123+1"), Some(123));
        assert_eq!(parse_job_id_field("123"), Some(123));
        assert_eq!(parse_job_id_field("123x"), None);
        assert_eq!(parse_job_id_field(""), None);
    }

    #[test]
    fn non_numeric_cpus_is_none() {
        let rows = parse_job_rows("1|c|n|p|e|l|end|N/A|m|t|S|r|s").unwrap();
        assert_eq!(rows[0].cpus, None);
    }

    #[test]
    fn gpus_from_tres_forms() {
        assert_eq!(gpus_from_tres("gres:gpu:a100:2"), 2);
        assert_eq!(gpus_from_tres("gres:gpu:2"), 2);
        assert_eq!(gpus_from_tres("gpu:4"), 4);
        assert_eq!(gpus_from_tres("gres:gpu:1"), 1);
        assert_eq!(gpus_from_tres("N/A"), 0);
        assert_eq!(gpus_from_tres(""), 0);
        assert_eq!(gpus_from_tres("gres:nvme:1"), 0);
        // No count: one GPU, as in the bash.
        assert_eq!(gpus_from_tres("gres:gpu"), 1);
        // Suffixes and newer spellings.
        assert_eq!(gpus_from_tres("gres:gpu:2(S:0-1)"), 2);
        assert_eq!(gpus_from_tres("gres/gpu:2"), 2);
        assert_eq!(gpus_from_tres("gres/gpu:a100:3"), 3);
        assert_eq!(gpus_from_tres("gres/gpu:a100=3"), 3);
        assert_eq!(gpus_from_tres("gres:nvme:1,gres:gpu:l40:8"), 8);
    }

    #[test]
    fn mem_to_mb_forms() {
        assert_eq!(mem_to_mb("32G"), Some(32768));
        assert_eq!(mem_to_mb("4000M"), Some(4000));
        assert_eq!(mem_to_mb("512"), Some(512));
        assert_eq!(mem_to_mb("1T"), Some(1_048_576));
        assert_eq!(mem_to_mb("2048K"), Some(2));
        assert_eq!(mem_to_mb("8Gn"), Some(8192));
        assert_eq!(mem_to_mb("2Gc"), Some(2048));
        assert_eq!(mem_to_mb("96GB"), Some(98304));
        assert_eq!(mem_to_mb("N/A"), None);
        assert_eq!(mem_to_mb(""), None);
        assert_eq!(mem_to_mb("32 G"), None);
        assert_eq!(mem_to_mb("-1G"), None);
        assert_eq!(mem_to_mb("1.5G"), None);
    }

    #[test]
    fn invalid_job_id_detection() {
        let e = SlurmError::Failed {
            cmd: "squeue".into(),
            status: 1,
            stderr: "slurm_load_jobs error: Invalid job id specified".into(),
        };
        assert!(is_invalid_job_id(&e));
        let e = SlurmError::Failed {
            cmd: "squeue".into(),
            status: 1,
            stderr: "slurm_load_jobs error: Unable to contact slurm controller".into(),
        };
        assert!(!is_invalid_job_id(&e));
    }

    #[test]
    fn missing_binary_is_not_found() {
        let s = Slurm {
            bin_dir: Some(std::path::PathBuf::from("/nonexistent/sint-test-bin")),
        };
        assert!(matches!(
            s.my_jobs(&["RUNNING"]),
            Err(SlurmError::NotFound { .. })
        ));
    }
}
