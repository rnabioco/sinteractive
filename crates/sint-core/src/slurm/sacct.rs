//! `sacct` / `sacctmgr` — accounting history and QOS limits.

use super::{Slurm, SlurmError};
use crate::time::slurm_timestamp_to_epoch;

/// One completed/failed job from `sacct -X -P -n`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AccountedJob {
    pub job_id: String,
    pub name: String,
    pub partition: String,
    pub state: String,
    pub elapsed: String,
    pub req_mem: String,
    pub max_rss: String,
    pub alloc_cpus: Option<u32>,
    pub end_epoch: Option<i64>,
}

pub const SACCT_FORMAT: &str = "JobID,JobName,Partition,State,Elapsed,ReqMem,MaxRSS,AllocCPUS,End";

/// Number of `|`-separated fields in [`SACCT_FORMAT`].
const SACCT_FIELDS: usize = 9;

/// Parse `sacct -X -P -n … --format=SACCT_FORMAT` output. Blank lines are
/// skipped; a row with too few fields is an error naming the line. A row
/// with extra fields is read as a job name containing `|` (the only
/// free-text column), and the surplus folds back into the name.
pub fn parse_sacct(output: &str) -> Result<Vec<AccountedJob>, SlurmError> {
    let mut jobs = Vec::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() < SACCT_FIELDS {
            return Err(SlurmError::Parse {
                cmd: "sacct".into(),
                reason: format!(
                    "expected {SACCT_FIELDS} fields, got {}: {line:?}",
                    fields.len()
                ),
            });
        }
        let extra = fields.len() - SACCT_FIELDS;
        // Columns after the name shift right by the surplus; JobID does not.
        let f = |i: usize| fields[i + extra].trim().to_string();
        jobs.push(AccountedJob {
            job_id: fields[0].trim().to_string(),
            name: fields[1..=1 + extra].join("|"),
            partition: f(2),
            state: f(3),
            elapsed: f(4),
            req_mem: f(5),
            max_rss: f(6),
            alloc_cpus: f(7).parse().ok(),
            end_epoch: slurm_timestamp_to_epoch(&f(8)),
        });
    }
    Ok(jobs)
}

impl Slurm {
    /// Recent jobs for the user since `since` (`now-1day` style).
    pub fn recent_jobs(&self, since: &str) -> Result<Vec<AccountedJob>, SlurmError> {
        let format = format!("--format={SACCT_FORMAT}");
        let mut args = vec!["-X", "-P", "-n"];
        // sacct defaults to the invoking user; name them explicitly when the
        // environment says who that is, as the bash-era scripts did.
        let user = std::env::var("USER").unwrap_or_default();
        if !user.is_empty() {
            args.extend(["-u", user.as_str()]);
        }
        args.extend(["--starttime", since, &format]);
        let out = self.run("sacct", &args)?;
        parse_sacct(&out)
    }

    /// `sacctmgr -nP show qos NAME format=MaxJobsPerUser` → limit, `None` when
    /// unset or unavailable (callers fail open).
    pub fn qos_max_jobs_per_user(&self, qos: &str) -> Option<u32> {
        let out = self
            .run(
                "sacctmgr",
                &["-nP", "show", "qos", qos, "format=MaxJobsPerUser"],
            )
            .ok()?;
        parse_max_jobs(&out)
    }
}

/// First non-empty line of `sacctmgr -nP … format=MaxJobsPerUser` as an
/// integer; an unset limit prints an empty line (or nothing at all).
fn parse_max_jobs(output: &str) -> Option<u32> {
    output
        .lines()
        .map(|l| l.trim().trim_end_matches('|').trim())
        .find(|l| !l.is_empty())?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
31757353|cargo-ci|rna|RUNNING|00:19:01|32G||8|Unknown
31757001|sint-mywork|interactive|COMPLETED|08:00:04|4000M|1234K|2|2026-08-27T22:00:04
31756990|weird|name|rna|FAILED|00:00:03|64G||16|2026-08-27T13:00:03

31756980|bash|rna|CANCELLED by 12345|00:10:00|8G||N/A|2026-08-27T12:10:00
";

    #[test]
    fn parses_fixture() {
        let jobs = parse_sacct(FIXTURE).unwrap();
        assert_eq!(jobs.len(), 4);

        let j = &jobs[0];
        assert_eq!(j.job_id, "31757353");
        assert_eq!(j.name, "cargo-ci");
        assert_eq!(j.partition, "rna");
        assert_eq!(j.state, "RUNNING");
        assert_eq!(j.elapsed, "00:19:01");
        assert_eq!(j.req_mem, "32G");
        assert_eq!(j.max_rss, "");
        assert_eq!(j.alloc_cpus, Some(8));
        assert_eq!(j.end_epoch, None);

        let j = &jobs[1];
        assert_eq!(j.max_rss, "1234K");
        assert_eq!(j.alloc_cpus, Some(2));
        assert_eq!(
            j.end_epoch,
            Some(slurm_timestamp_to_epoch("2026-08-27T22:00:04").unwrap())
        );

        // A '|' in the job name folds back into the name.
        let j = &jobs[2];
        assert_eq!(j.name, "weird|name");
        assert_eq!(j.partition, "rna");
        assert_eq!(j.state, "FAILED");
        assert_eq!(j.alloc_cpus, Some(16));

        let j = &jobs[3];
        assert_eq!(j.state, "CANCELLED by 12345");
        assert_eq!(j.alloc_cpus, None);
    }

    #[test]
    fn empty_is_no_jobs() {
        assert!(parse_sacct("").unwrap().is_empty());
        assert!(parse_sacct("\n\n").unwrap().is_empty());
    }

    #[test]
    fn short_row_is_error() {
        assert!(matches!(
            parse_sacct("1|a|b\n"),
            Err(SlurmError::Parse { .. })
        ));
    }

    #[test]
    fn max_jobs_parsing() {
        assert_eq!(parse_max_jobs("2\n"), Some(2));
        assert_eq!(parse_max_jobs("2|\n"), Some(2));
        assert_eq!(parse_max_jobs("  12  "), Some(12));
        assert_eq!(parse_max_jobs("\n"), None);
        assert_eq!(parse_max_jobs(""), None);
        assert_eq!(parse_max_jobs("unlimited"), None);
    }

    #[test]
    fn missing_binary() {
        let s = Slurm {
            bin_dir: Some(std::path::PathBuf::from("/nonexistent/sint-test-bin")),
        };
        assert_eq!(s.qos_max_jobs_per_user("interactive"), None);
        assert!(matches!(
            s.recent_jobs("now-1day"),
            Err(SlurmError::NotFound { .. })
        ));
    }
}
