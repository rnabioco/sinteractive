//! Session notices: `<jobid>.notices`, TSV `kind\ttext`, one per line.
//!
//! Colour belongs to renderers; the file stays greppable. The file is
//! **removed** when there are no notices, so absence means "nothing to say".
//! Producers (script lines 1499-1600): quota overage, maintenance-trimmed end
//! time, and the Claude Code install hint (gated on a live `claude` process
//! and the integration not yet installed).

use std::fs;
use std::io;

use serde::{Deserialize, Serialize};

use crate::state::{atomic_write, StateDir};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Notice {
    /// `quota`, `maint`, `hint`, … — renderers pick severity from this.
    pub kind: String,
    pub text: String,
}

impl Notice {
    pub fn new(kind: &str, text: impl Into<String>) -> Self {
        Notice {
            kind: kind.to_string(),
            text: text.into(),
        }
    }
    /// Quota notices are severe (red + shimmer); everything else is a warning.
    pub fn is_severe(&self) -> bool {
        self.kind == "quota"
    }
}

/// Parse the TSV file contents. Blank lines and lines without a tab (or with
/// an empty kind) are skipped rather than failing the whole file.
pub fn parse_notices(tsv: &str) -> Vec<Notice> {
    tsv.lines()
        .filter_map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.trim().is_empty() {
                return None;
            }
            let (kind, text) = line.split_once('\t')?;
            let kind = kind.trim();
            if kind.is_empty() {
                return None;
            }
            Some(Notice::new(kind, text))
        })
        .collect()
}

/// Serialise to TSV (trailing newline; empty vec → empty string).
pub fn to_tsv(notices: &[Notice]) -> String {
    let mut out = String::new();
    for n in notices {
        out.push_str(&n.kind);
        out.push('\t');
        out.push_str(&n.text);
        out.push('\n');
    }
    out
}

/// Read notices for a job; missing file → empty.
pub fn read(dir: &StateDir, job_id: u64) -> Vec<Notice> {
    fs::read_to_string(dir.notices_file(job_id))
        .map(|s| parse_notices(&s))
        .unwrap_or_default()
}

/// Write notices atomically; empty → remove the file (and any stray `.tmp`).
pub fn write(dir: &StateDir, job_id: u64, notices: &[Notice]) -> io::Result<()> {
    let path = dir.notices_file(job_id);
    if notices.is_empty() {
        let mut tmp = path.clone().into_os_string();
        tmp.push(".tmp");
        for p in [path, tmp.into()] {
            match fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }
    atomic_write(&path, to_tsv(notices).as_bytes())
}

/// `QUOTA over by X (Y limit)` — the overage, because that is the number you
/// act on.
pub fn quota_notice(over_kb: u64, hard_kb: u64) -> Notice {
    Notice::new(
        "quota",
        format!(
            "QUOTA over by {} ({} limit)",
            crate::quota::kb_to_size(over_kb),
            crate::quota::kb_to_size(hard_kb)
        ),
    )
}

/// `Session ends <date> — trimmed to finish before maintenance (<resv>)`.
/// The date is local time as `%a %b %-d %H:%M` (`Thu Sep 3 07:55`), the
/// exact form the 0.x script printed, so `--status` output is unchanged.
pub fn maint_notice(end_epoch: i64, reservation: &str) -> Notice {
    Notice::new(
        "maint",
        format!(
            "Session ends {} — trimmed to finish before maintenance ({reservation})",
            format_local_datetime(end_epoch)
        ),
    )
}

/// `Claude Code: run sinteractive claude install to enable the skills and hooks`.
pub fn claude_hint_notice() -> Notice {
    Notice::new(
        "hint",
        "Claude Code: run sinteractive claude install to enable the skills and hooks",
    )
}

/// The local UTC offset in force at `epoch`, via `localtime_r(3)` — the same
/// answer `date` gives, including DST at that instant, and unaffected by the
/// `time` crate's refusal to read the zone in a multi-threaded process.
/// Falls back to the `time` crate, then to UTC.
fn local_offset_at(epoch: i64) -> ::time::UtcOffset {
    let t: libc::time_t = epoch as libc::time_t;
    // SAFETY: tm is plain data; localtime_r fills it or returns null.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if ok {
        if let Ok(off) = ::time::UtcOffset::from_whole_seconds(tm.tm_gmtoff as i32) {
            return off;
        }
    }
    ::time::UtcOffset::current_local_offset().unwrap_or(::time::UtcOffset::UTC)
}

/// `%a %b %-d %H:%M` in local time; UTC when the zone cannot be determined.
pub fn format_local_datetime(epoch: i64) -> String {
    let Ok(utc) = ::time::OffsetDateTime::from_unix_timestamp(epoch) else {
        return epoch.to_string();
    };
    let local = utc.to_offset(local_offset_at(epoch));
    let fmt = ::time::macros::format_description!(
        "[weekday repr:short] [month repr:short] [day padding:none] [hour]:[minute]"
    );
    local.format(&fmt).unwrap_or_else(|_| epoch.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skips_blank_and_malformed_lines() {
        let tsv = "quota\tQUOTA over by 1G (5G limit)\n\n\r\nnotab line\n\tno kind\nmaint\tSession ends soon\r\nhint\t\n";
        let got = parse_notices(tsv);
        assert_eq!(
            got,
            vec![
                Notice::new("quota", "QUOTA over by 1G (5G limit)"),
                Notice::new("maint", "Session ends soon"),
                Notice::new("hint", ""),
            ]
        );
        assert!(parse_notices("").is_empty());
    }

    #[test]
    fn tsv_round_trip() {
        let notices = vec![
            quota_notice(13212057, 524288000),
            Notice::new("maint", "Session ends Thu Sep 3 07:55 — trimmed"),
            claude_hint_notice(),
        ];
        let tsv = to_tsv(&notices);
        assert!(tsv.ends_with('\n'));
        assert_eq!(tsv.lines().count(), 3);
        assert_eq!(parse_notices(&tsv), notices);
        assert_eq!(to_tsv(&[]), "");
    }

    #[test]
    fn file_round_trip_and_removal_when_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().join("cache"));
        assert!(read(&sd, 42).is_empty(), "missing file reads empty");

        let notices = vec![quota_notice(1024, 1048576), claude_hint_notice()];
        write(&sd, 42, &notices).expect("write");
        let path = sd.notices_file(42);
        assert!(path.exists());
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "quota\tQUOTA over by 1M (1G limit)\nhint\tClaude Code: run sinteractive claude install to enable the skills and hooks\n"
        );
        assert_eq!(read(&sd, 42), notices);

        write(&sd, 42, &[]).expect("write empty");
        assert!(!path.exists(), "empty removes the file");
        assert!(read(&sd, 42).is_empty());
        // Removing again is not an error.
        write(&sd, 42, &[]).expect("write empty twice");
    }

    #[test]
    fn quota_notice_wording() {
        let n = quota_notice(12897485, 524288000);
        assert!(n.is_severe());
        assert_eq!(n.kind, "quota");
        assert_eq!(n.text, "QUOTA over by 12.3G (500G limit)");
    }

    #[test]
    fn maint_notice_wording() {
        // 2026-09-03 07:55:00 UTC.
        let epoch = 1788422100;
        let n = maint_notice(epoch, "maint-2026-09");
        assert!(!n.is_severe());
        assert_eq!(n.kind, "maint");
        assert!(n.text.starts_with("Session ends "), "{}", n.text);
        assert!(
            n.text
                .ends_with(" — trimmed to finish before maintenance (maint-2026-09)"),
            "{}",
            n.text
        );
        // The date part has the `%a %b %-d %H:%M` shape whatever the zone.
        let date = n
            .text
            .trim_start_matches("Session ends ")
            .split(" — ")
            .next()
            .unwrap();
        let parts: Vec<&str> = date.split(' ').collect();
        assert_eq!(parts.len(), 4, "{date}");
        assert_eq!(parts[0].len(), 3);
        assert_eq!(parts[1].len(), 3);
        assert!(parts[2].len() == 1 || parts[2].len() == 2, "{date}");
        assert!(!parts[2].starts_with('0'), "day is unpadded: {date}");
        assert_eq!(parts[3].len(), 5);
    }

    #[test]
    fn format_local_datetime_in_utc() {
        // With no zone the result is UTC and exact.
        let off = local_offset_at(1788422100);
        if off == ::time::UtcOffset::UTC {
            assert_eq!(format_local_datetime(1788422100), "Thu Sep 3 07:55");
        }
        let s = format_local_datetime(1788422100);
        assert!(s.starts_with("Thu ") || s.starts_with("Wed "), "{s}");
    }

    #[test]
    fn hint_notice_is_a_warning() {
        assert!(!claude_hint_notice().is_severe());
    }
}
