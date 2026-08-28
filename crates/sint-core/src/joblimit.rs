//! Per-QOS concurrent job cap (script lines 2072-2150).
//!
//! 0.x hardcoded partition and QOS `interactive`; here the check runs for
//! whatever QOS the launch resolves to (`--qos`, `SINTERACTIVE_QOS`, or the
//! partition name as a last guess) and fails open when `sacctmgr`/`squeue`
//! are unavailable.

use crate::slurm::squeue::JobRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitHit {
    pub qos: String,
    pub limit: u32,
    /// Jobs counted against the limit (RUNNING + PENDING in that partition/QOS).
    pub jobs: Vec<JobRow>,
}

/// `Some(hit)` when submitting one more job would exceed `limit`.
///
/// `limit` is `None` when `sacctmgr` had nothing to say — fail open, as the
/// bash did. The count is the user's RUNNING and PENDING jobs in
/// `partition` (a PENDING job holds a slot just as a RUNNING one does);
/// `rows` may hold every job the user has, the filter is applied here. A
/// limit of 0 means the QOS is unlimited in Slurm's accounting and is
/// treated as no limit.
pub fn check(qos: &str, limit: Option<u32>, rows: &[JobRow], partition: &str) -> Option<LimitHit> {
    let limit = limit.filter(|l| *l > 0)?;
    let jobs: Vec<JobRow> = rows
        .iter()
        .filter(|r| r.partition == partition)
        .filter(|r| matches!(r.state.as_str(), "RUNNING" | "PENDING"))
        .cloned()
        .collect();
    if jobs.len() as u64 >= u64::from(limit) {
        Some(LimitHit {
            qos: qos.to_string(),
            limit,
            jobs,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(job_id: u64, partition: &str, state: &str) -> JobRow {
        JobRow {
            job_id,
            partition: partition.into(),
            state: state.into(),
            ..JobRow::default()
        }
    }

    #[test]
    fn no_limit_fails_open() {
        let rows = vec![row(1, "interactive", "RUNNING")];
        assert_eq!(check("interactive", None, &rows, "interactive"), None);
        assert_eq!(check("interactive", Some(0), &rows, "interactive"), None);
    }

    #[test]
    fn under_limit_is_fine() {
        let rows = vec![row(1, "interactive", "RUNNING")];
        assert_eq!(check("interactive", Some(2), &rows, "interactive"), None);
        assert_eq!(check("interactive", Some(1), &[], "interactive"), None);
    }

    #[test]
    fn at_limit_is_a_hit_with_the_counted_jobs() {
        let rows = vec![
            row(1, "interactive", "RUNNING"),
            row(2, "interactive", "PENDING"),
            row(3, "rna", "RUNNING"),
            row(4, "interactive", "COMPLETING"),
        ];
        let hit = check("interactive", Some(2), &rows, "interactive").unwrap();
        assert_eq!(hit.qos, "interactive");
        assert_eq!(hit.limit, 2);
        let ids: Vec<u64> = hit.jobs.iter().map(|j| j.job_id).collect();
        assert_eq!(ids, vec![1, 2]);
        // Other partitions are not counted.
        assert_eq!(check("rna", Some(2), &rows, "rna"), None);
    }

    #[test]
    fn over_limit_is_a_hit_too() {
        let rows = vec![
            row(1, "interactive", "RUNNING"),
            row(2, "interactive", "RUNNING"),
            row(3, "interactive", "PENDING"),
        ];
        let hit = check("interactive", Some(2), &rows, "interactive").unwrap();
        assert_eq!(hit.jobs.len(), 3);
    }
}
