//! The session JSON contract and session discovery.
//!
//! **Frozen contract** (docs/scripting.md). Field order is the serialisation
//! order below. `cwd` appears only from `list`; `created` only from `ensure`.
//! `end_epoch` and `remaining_seconds` go `null` together. `gpus` is always an
//! integer. `cpus` is what Slurm *allocated*, which can exceed the request.
//!
//! Identity: a session is a Slurm job whose Comment is exactly `sinteractive`
//! or `sinteractive:NAME`. The job Name (`sint-NAME`) is decorative.

use serde::{Deserialize, Serialize};

use crate::slurm::squeue::{gpus_from_tres, mem_to_mb, JobRow};
use crate::time::slurm_timestamp_to_epoch;

/// Comment marker prefix.
pub const COMMENT_BASE: &str = "sinteractive";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionInfo {
    pub job_id: u64,
    pub name: Option<String>,
    pub state: String,
    pub node: Option<String>,
    pub partition: Option<String>,
    pub cpus: Option<u32>,
    pub memory: Option<String>,
    pub memory_mb: Option<u64>,
    pub gpus: u32,
    pub time_limit: Option<String>,
    pub elapsed: Option<String>,
    pub end_epoch: Option<i64>,
    pub remaining_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<bool>,
}

impl SessionInfo {
    /// The `{"job_id":N,"state":"NOT_FOUND"}` object (exit 1 at the CLI).
    pub fn not_found(job_id: u64) -> serde_json::Value {
        serde_json::json!({"job_id": job_id, "state": "NOT_FOUND"})
    }

    /// Build from an `squeue` row at time `now`.
    ///
    /// Mirrors `show_status` (script line 1012): empty strings become
    /// `null`; the raw memory string is kept even when it is `N/A` (then
    /// `memory_mb` is `null`); `end_epoch`/`remaining_seconds` are only
    /// filled for a RUNNING job, since `%e` on a pending job is a guess,
    /// and go `null` together.
    pub fn from_row(row: &JobRow, now: i64) -> Self {
        let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
        let name = parse_comment(&row.comment).flatten();
        let end_epoch = if row.state == "RUNNING" {
            slurm_timestamp_to_epoch(&row.end_time)
        } else {
            None
        };
        let remaining_seconds = end_epoch.map(|e| (e - now).max(0));
        SessionInfo {
            job_id: row.job_id,
            name,
            state: row.state.clone(),
            node: opt(&row.node),
            partition: opt(&row.partition),
            cpus: row.cpus,
            memory: opt(&row.min_memory),
            memory_mb: mem_to_mb(&row.min_memory),
            gpus: gpus_from_tres(&row.tres_per_node),
            time_limit: opt(&row.time_limit),
            elapsed: opt(&row.elapsed),
            end_epoch,
            remaining_seconds,
            cwd: None,
            created: None,
        }
    }
}

/// `sinteractive` / `sinteractive:NAME` → `Some(None)` / `Some(Some(NAME))`;
/// any other comment → `None`.
///
/// A bare `sinteractive:` (empty name) counts as an unnamed session: the
/// bash `--list` filter (`index($2, "sinteractive:") == 1`) admitted it and
/// its name regex (`^sinteractive:(.+)$`) then found no name.
pub fn parse_comment(comment: &str) -> Option<Option<String>> {
    if comment == COMMENT_BASE {
        return Some(None);
    }
    let name = comment.strip_prefix(COMMENT_BASE)?.strip_prefix(':')?;
    Some((!name.is_empty()).then(|| name.to_string()))
}

/// Comment for a session with an optional name.
pub fn comment_for(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{COMMENT_BASE}:{n}"),
        None => COMMENT_BASE.to_string(),
    }
}

/// Session names: `^[A-Za-z0-9._-]+$`.
pub fn validate_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err("name must contain only letters, digits, '.', '_', '-'".to_string())
    }
}

/// A target given on the command line: a job id or a session name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    JobId(u64),
    Name(String),
}

impl Target {
    pub fn parse(s: &str) -> Target {
        match s.parse::<u64>() {
            Ok(id) => Target::JobId(id),
            Err(_) => Target::Name(s.to_string()),
        }
    }
}

/// Resolve a target to a job id against `rows` (RUNNING+PENDING sessions).
/// Errors on zero or more than one name match (script line 2152).
///
/// A job id passes through untouched — the bash did not check that it
/// exists here; the next squeue call reports that. A name must match the
/// comment `sinteractive:NAME` of exactly one RUNNING or PENDING row.
pub fn resolve_target(target: &Target, rows: &[JobRow]) -> Result<u64, String> {
    let name = match target {
        Target::JobId(id) => return Ok(*id),
        Target::Name(n) => n,
    };
    let wanted = comment_for(Some(name));
    let matches: Vec<u64> = rows
        .iter()
        .filter(|r| matches!(r.state.as_str(), "RUNNING" | "PENDING"))
        .filter(|r| r.comment == wanted)
        .map(|r| r.job_id)
        .collect();
    match matches.as_slice() {
        [] => Err(format!("no sinteractive session named '{name}'")),
        [id] => Ok(*id),
        ids => {
            let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
            Err(format!(
                "multiple sinteractive sessions named '{name}': {}",
                ids.join(" ")
            ))
        }
    }
}

/// Filter rows to sinteractive sessions.
pub fn sessions_only(rows: &[JobRow]) -> Vec<&JobRow> {
    rows.iter()
        .filter(|r| parse_comment(&r.comment).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(job_id: u64, comment: &str, state: &str) -> JobRow {
        JobRow {
            job_id,
            comment: comment.to_string(),
            state: state.to_string(),
            ..JobRow::default()
        }
    }

    fn running_row() -> JobRow {
        JobRow {
            job_id: 147845,
            comment: "sinteractive:mywork".into(),
            node: "compute20".into(),
            partition: "rna".into(),
            elapsed: "0:43".into(),
            time_limit: "8:00:00".into(),
            end_time: "2026-07-04T12:02:32".into(),
            cpus: Some(8),
            min_memory: "32G".into(),
            tres_per_node: "N/A".into(),
            state: "RUNNING".into(),
            reason: "None".into(),
            start_time: "2026-07-04T04:02:32".into(),
        }
    }

    #[test]
    fn serialises_the_frozen_contract() {
        let r = running_row();
        // The contract example: end_epoch 1783180952, remaining 28757.
        let end = slurm_timestamp_to_epoch(&r.end_time).unwrap();
        let now = end - 28757;
        let mut info = SessionInfo::from_row(&r, now);
        // Pin the epoch so the golden string holds in every time zone.
        assert_eq!(info.remaining_seconds, Some(28757));
        info.end_epoch = Some(1783180952);
        let json = serde_json::to_string(&info).unwrap();
        assert_eq!(
            json,
            r#"{"job_id":147845,"name":"mywork","state":"RUNNING","node":"compute20","partition":"rna","cpus":8,"memory":"32G","memory_mb":32768,"gpus":0,"time_limit":"8:00:00","elapsed":"0:43","end_epoch":1783180952,"remaining_seconds":28757}"#
        );
    }

    #[test]
    fn unnamed_session_has_null_name() {
        let mut r = running_row();
        r.comment = "sinteractive".into();
        let info = SessionInfo::from_row(&r, 0);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.starts_with(r#"{"job_id":147845,"name":null,"state":"RUNNING""#));
        assert!(!json.contains("\"cwd\""));
        assert!(!json.contains("\"created\""));
    }

    #[test]
    fn cwd_and_created_appear_only_when_set() {
        let mut info = SessionInfo::from_row(&running_row(), 0);
        info.cwd = Some("~/proj".into());
        info.created = Some(true);
        let json = serde_json::to_string(&info).unwrap();
        assert!(
            json.ends_with(r#","cwd":"~/proj","created":true}"#),
            "{json}"
        );
    }

    #[test]
    fn pending_row_has_null_budget_and_node() {
        let r = JobRow {
            job_id: 147850,
            comment: "sinteractive:queued".into(),
            node: "".into(),
            partition: "interactive".into(),
            elapsed: "0:00".into(),
            time_limit: "4:00:00".into(),
            end_time: "N/A".into(),
            cpus: Some(2),
            min_memory: "N/A".into(),
            tres_per_node: "gres:gpu:1".into(),
            state: "PENDING".into(),
            reason: "Priority".into(),
            start_time: "N/A".into(),
        };
        let info = SessionInfo::from_row(&r, 0);
        assert_eq!(info.node, None);
        assert_eq!(info.end_epoch, None);
        assert_eq!(info.remaining_seconds, None);
        assert_eq!(info.memory.as_deref(), Some("N/A"));
        assert_eq!(info.memory_mb, None);
        assert_eq!(info.gpus, 1);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""node":null"#));
        assert!(json.contains(r#""end_epoch":null,"remaining_seconds":null"#));
    }

    #[test]
    fn pending_row_ignores_estimated_end() {
        let mut r = running_row();
        r.state = "PENDING".into();
        let info = SessionInfo::from_row(&r, 0);
        assert_eq!(info.end_epoch, None);
    }

    #[test]
    fn remaining_clamps_at_zero() {
        let r = running_row();
        let end = slurm_timestamp_to_epoch(&r.end_time).unwrap();
        let info = SessionInfo::from_row(&r, end + 100);
        assert_eq!(info.remaining_seconds, Some(0));
        assert_eq!(info.end_epoch, Some(end));
    }

    #[test]
    fn unknown_end_time_goes_null_together() {
        let mut r = running_row();
        r.end_time = "Unknown".into();
        let info = SessionInfo::from_row(&r, 0);
        assert_eq!(info.end_epoch, None);
        assert_eq!(info.remaining_seconds, None);
    }

    #[test]
    fn not_found_shape() {
        assert_eq!(
            SessionInfo::not_found(7).to_string(),
            r#"{"job_id":7,"state":"NOT_FOUND"}"#
        );
    }

    #[test]
    fn parse_comment_forms() {
        assert_eq!(parse_comment("sinteractive"), Some(None));
        assert_eq!(
            parse_comment("sinteractive:mywork"),
            Some(Some("mywork".into()))
        );
        assert_eq!(parse_comment("sinteractive:"), Some(None));
        assert_eq!(parse_comment("sinteractivex"), None);
        assert_eq!(parse_comment("sinteractive mywork"), None);
        assert_eq!(parse_comment("xsinteractive:mywork"), None);
        assert_eq!(parse_comment(""), None);
        assert_eq!(parse_comment("make-test"), None);
    }

    #[test]
    fn comment_roundtrip() {
        assert_eq!(comment_for(None), "sinteractive");
        assert_eq!(comment_for(Some("a.b_c-1")), "sinteractive:a.b_c-1");
        assert_eq!(
            parse_comment(&comment_for(Some("x"))),
            Some(Some("x".into()))
        );
    }

    #[test]
    fn validate_name_rules() {
        assert!(validate_name("mywork").is_ok());
        assert!(validate_name("a.b_c-1").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("my work").is_err());
        assert!(validate_name("my/work").is_err());
        assert!(validate_name("naïve").is_err());
        assert!(validate_name("a:b").is_err());
    }

    #[test]
    fn target_parse() {
        assert_eq!(Target::parse("123"), Target::JobId(123));
        assert_eq!(Target::parse("mywork"), Target::Name("mywork".into()));
        assert_eq!(Target::parse("12a"), Target::Name("12a".into()));
    }

    #[test]
    fn resolve_numeric_passes_through_unchecked() {
        assert_eq!(resolve_target(&Target::JobId(999), &[]), Ok(999));
    }

    #[test]
    fn resolve_by_name() {
        let rows = vec![
            row(1, "sinteractive:alpha", "RUNNING"),
            row(2, "sinteractive:beta", "PENDING"),
            row(3, "sinteractive", "RUNNING"),
            row(4, "sinteractive:gamma", "COMPLETING"),
            row(5, "make-test", "RUNNING"),
        ];
        assert_eq!(resolve_target(&Target::Name("alpha".into()), &rows), Ok(1));
        assert_eq!(resolve_target(&Target::Name("beta".into()), &rows), Ok(2));
        let err = resolve_target(&Target::Name("gamma".into()), &rows).unwrap_err();
        assert_eq!(err, "no sinteractive session named 'gamma'");
        let err = resolve_target(&Target::Name("nope".into()), &rows).unwrap_err();
        assert_eq!(err, "no sinteractive session named 'nope'");
        // Prefix is not a match.
        assert!(resolve_target(&Target::Name("alph".into()), &rows).is_err());
    }

    #[test]
    fn resolve_ambiguous_name() {
        let rows = vec![
            row(10, "sinteractive:dup", "RUNNING"),
            row(11, "sinteractive:dup", "PENDING"),
        ];
        let err = resolve_target(&Target::Name("dup".into()), &rows).unwrap_err();
        assert_eq!(err, "multiple sinteractive sessions named 'dup': 10 11");
    }

    #[test]
    fn sessions_only_filters_by_comment() {
        let rows = vec![
            row(1, "sinteractive:alpha", "RUNNING"),
            row(2, "cargo-ci", "RUNNING"),
            row(3, "sinteractive", "RUNNING"),
        ];
        let ids: Vec<u64> = sessions_only(&rows).iter().map(|r| r.job_id).collect();
        assert_eq!(ids, vec![1, 3]);
    }
}
