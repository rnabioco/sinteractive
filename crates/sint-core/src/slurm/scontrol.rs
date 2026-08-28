//! `scontrol` — job comment tagging, reservations, cluster config.

use super::{Slurm, SlurmError};
use crate::time::slurm_timestamp_to_epoch;

/// One reservation from `scontrol show reservation -o`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    pub name: String,
    pub start_epoch: i64,
    pub end_epoch: i64,
    pub flags: Vec<String>,
    pub nodes: String,
    pub users: String,
}

/// Parse `scontrol show reservation -o` (one `Key=Value …` line per
/// reservation). Timestamps are local time `YYYY-MM-DDTHH:MM:SS`.
///
/// A line without `ReservationName=` is an error; a reservation whose
/// `StartTime`/`EndTime` do not parse is skipped, as the bash did
/// (`next_maintenance_window` treated it as "no window" rather than failing
/// the launch). `(null)` values read as empty strings.
pub fn parse_reservations(output: &str) -> Result<Vec<Reservation>, SlurmError> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("No reservations") {
            continue;
        }
        let get = |key: &str| -> Option<&str> {
            line.split_whitespace()
                .find_map(|tok| tok.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
        };
        let name = get("ReservationName").ok_or_else(|| SlurmError::Parse {
            cmd: "scontrol".into(),
            reason: format!("no ReservationName in {line:?}"),
        })?;
        let (Some(start), Some(end)) = (
            get("StartTime").and_then(slurm_timestamp_to_epoch),
            get("EndTime").and_then(slurm_timestamp_to_epoch),
        ) else {
            continue;
        };
        let clean = |v: Option<&str>| match v {
            None | Some("(null)") => String::new(),
            Some(v) => v.to_string(),
        };
        let flags = get("Flags")
            .filter(|f| *f != "(null)")
            .map(|f| {
                f.split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push(Reservation {
            name: name.to_string(),
            start_epoch: start,
            end_epoch: end,
            flags,
            nodes: clean(get("Nodes")),
            users: clean(get("Users")),
        });
    }
    Ok(out)
}

impl Slurm {
    /// `scontrol update JobId=ID Comment=…`.
    pub fn set_comment(&self, job_id: u64, comment: &str) -> Result<(), SlurmError> {
        let job = format!("JobId={job_id}");
        let comment = format!("Comment={comment}");
        self.run("scontrol", &["update", &job, &comment])?;
        Ok(())
    }

    pub fn reservations(&self) -> Result<Vec<Reservation>, SlurmError> {
        let out = self.run("scontrol", &["show", "reservation", "-o"])?;
        parse_reservations(&out)
    }

    /// `scontrol show config` → `ClusterName`, or `None` when unavailable.
    pub fn cluster_name(&self) -> Option<String> {
        let out = self.run("scontrol", &["show", "config"]).ok()?;
        cluster_name_from_config(&out)
    }
}

/// The `ClusterName = alpine` line of `scontrol show config`.
fn cluster_name_from_config(config: &str) -> Option<String> {
    config.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        if k.trim() != "ClusterName" {
            return None;
        }
        let v = v.trim();
        (!v.is_empty() && v != "(null)").then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAINT_LINE: &str = "ReservationName=maint-2026-09 StartTime=2026-09-03T08:00:00 EndTime=2026-09-03T20:00:00 Duration=12:00:00 Nodes=ALL NodeCnt=6 CoreCnt=384 Features=(null) PartitionName=(null) Flags=MAINT,IGNORE_JOBS,SPEC_NODES,ALL_NODES TRES=cpu=384 Users=root Groups=(null) Accounts=(null) Licenses=(null) State=INACTIVE BurstBuffer=(null) MaxStartDelay=(null)";

    #[test]
    fn parses_real_maint_line() {
        let r = parse_reservations(MAINT_LINE).unwrap();
        assert_eq!(r.len(), 1);
        let r = &r[0];
        assert_eq!(r.name, "maint-2026-09");
        assert_eq!(
            r.start_epoch,
            slurm_timestamp_to_epoch("2026-09-03T08:00:00").unwrap()
        );
        assert_eq!(
            r.end_epoch,
            slurm_timestamp_to_epoch("2026-09-03T20:00:00").unwrap()
        );
        assert_eq!(r.end_epoch - r.start_epoch, 12 * 3600);
        assert_eq!(
            r.flags,
            vec!["MAINT", "IGNORE_JOBS", "SPEC_NODES", "ALL_NODES"]
        );
        assert_eq!(r.nodes, "ALL");
        assert_eq!(r.users, "root");
    }

    #[test]
    fn parses_multiple_lines_and_nulls() {
        let input = format!(
            "{MAINT_LINE}\n\
             ReservationName=rna-course StartTime=2026-09-10T09:00:00 EndTime=2026-09-10T17:00:00 Nodes=c3cpu-a2-u1-[1-4] Flags=(null) Users=(null) Accounts=rnabioco\n\
             \n"
        );
        let r = parse_reservations(&input).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].name, "rna-course");
        assert_eq!(r[1].nodes, "c3cpu-a2-u1-[1-4]");
        assert!(r[1].flags.is_empty());
        assert_eq!(r[1].users, "");
    }

    #[test]
    fn empty_and_none_outputs_are_empty() {
        assert!(parse_reservations("").unwrap().is_empty());
        assert!(parse_reservations("No reservations in the system\n")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unparseable_timestamp_is_skipped() {
        let input = "ReservationName=odd StartTime=Unknown EndTime=Unknown Flags=MAINT\n";
        assert!(parse_reservations(input).unwrap().is_empty());
    }

    #[test]
    fn line_without_name_is_error() {
        let input = "StartTime=2026-09-03T08:00:00 EndTime=2026-09-03T20:00:00\n";
        assert!(matches!(
            parse_reservations(input),
            Err(SlurmError::Parse { .. })
        ));
    }

    #[test]
    fn cluster_name_line() {
        let cfg = "Configuration data as of 2026-08-28T10:00:00\n\
                   AccountingStorageType  = accounting_storage/slurmdbd\n\
                   ClusterName             = alpine\n\
                   ClusterNameX            = nope\n";
        assert_eq!(cluster_name_from_config(cfg).as_deref(), Some("alpine"));
        assert_eq!(cluster_name_from_config("ClusterName = (null)\n"), None);
        assert_eq!(cluster_name_from_config(""), None);
    }

    #[test]
    fn missing_binary() {
        let s = Slurm {
            bin_dir: Some(std::path::PathBuf::from("/nonexistent/sint-test-bin")),
        };
        assert_eq!(s.cluster_name(), None);
        assert!(matches!(s.reservations(), Err(SlurmError::NotFound { .. })));
        assert!(matches!(
            s.set_comment(1, "sinteractive"),
            Err(SlurmError::NotFound { .. })
        ));
    }
}
