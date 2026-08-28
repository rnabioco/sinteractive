//! Maintenance reservations (script lines 1971-2070).
//!
//! A request that would overlap the next `MAINT` reservation is **shortened**
//! to end `MAINT_MARGIN` before it; if that leaves under `MAINT_MIN_SESSION`
//! the launch is refused. An explicit `--reservation` skips the check. The
//! trimmed end is carried into the session as `--maint=NAME@EPOCH` so the
//! notice says the same thing all session long even if the reservation moves.

use crate::slurm::scontrol::Reservation;

pub const MAINT_MARGIN: i64 = 300;
pub const MAINT_MIN_SESSION: i64 = 600;

/// The earliest future reservation carrying a `MAINT` flag.
///
/// "Future" is `start_epoch > now`; a maintenance already under way is not
/// something a new request can be fitted before. Ties keep the first listed.
pub fn next_maintenance(reservations: &[Reservation], now: i64) -> Option<Reservation> {
    reservations
        .iter()
        .filter(|r| r.flags.iter().any(|f| f.eq_ignore_ascii_case("MAINT")))
        .filter(|r| r.start_epoch > now)
        .min_by_key(|r| r.start_epoch)
        .cloned()
}

/// Result of fitting a request of `requested_secs` starting at `now`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fit {
    /// No overlap; submit as requested.
    Unchanged,
    /// Overlap; submit with this walltime (seconds) and carry the notice.
    Trimmed {
        secs: i64,
        reservation: String,
        ends_epoch: i64,
    },
    /// Under `MAINT_MIN_SESSION` after trimming; refuse.
    Refuse {
        reservation: String,
        starts_epoch: i64,
    },
}

/// Fit `requested_secs` before `next` (from [`next_maintenance`]).
///
/// `Trimmed::ends_epoch` is when the shortened session ends (`now + secs`,
/// i.e. `MAINT_MARGIN` before the reservation), not the reservation start:
/// the bash carried the start and then announced it as "Session ends …",
/// which was off by the margin.
pub fn fit(requested_secs: i64, now: i64, next: Option<&Reservation>) -> Fit {
    let Some(next) = next else {
        return Fit::Unchanged;
    };
    let job_end = now + requested_secs;
    if job_end <= next.start_epoch {
        return Fit::Unchanged;
    }
    let gap = next.start_epoch - now;
    let secs = gap - MAINT_MARGIN;
    if secs < MAINT_MIN_SESSION {
        return Fit::Refuse {
            reservation: next.name.clone(),
            starts_epoch: next.start_epoch,
        };
    }
    Fit::Trimmed {
        secs,
        reservation: next.name.clone(),
        ends_epoch: now + secs,
    }
}

/// Rewrite `--time`/`-t` in an sbatch arg list (`--time X`, `--time=X`, `-t X`, `-tX`).
///
/// Every occurrence is rewritten (sbatch takes the last one, so all must
/// agree). Returns whether anything was replaced; when nothing was, the
/// caller has to append the walltime itself. `-t=X` is accepted too, as the
/// bash did.
pub fn replace_time_in_args(args: &mut [String], new_time: &str) -> bool {
    let mut replaced = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--time" || a == "-t" {
            if i + 1 < args.len() {
                args[i + 1] = new_time.to_string();
                replaced = true;
                i += 2;
                continue;
            }
        } else if a.starts_with("--time=") {
            args[i] = format!("--time={new_time}");
            replaced = true;
        } else if a.starts_with("-t=") {
            args[i] = format!("-t={new_time}");
            replaced = true;
        } else if a.starts_with("-t") && a.len() > 2 && !a.starts_with("--") {
            args[i] = format!("-t{new_time}");
            replaced = true;
        }
        i += 1;
    }
    replaced
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resv(name: &str, start: i64, flags: &[&str]) -> Reservation {
        Reservation {
            name: name.into(),
            start_epoch: start,
            end_epoch: start + 12 * 3600,
            flags: flags.iter().map(|s| s.to_string()).collect(),
            nodes: "ALL".into(),
            users: "root".into(),
        }
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn next_maintenance_picks_earliest_future_maint() {
        let rs = vec![
            resv("past", NOW - 10, &["MAINT"]),
            resv("later", NOW + 7200, &["MAINT", "IGNORE_JOBS"]),
            resv("course", NOW + 60, &["SPEC_NODES"]),
            resv("soon", NOW + 3600, &["IGNORE_JOBS", "MAINT"]),
        ];
        assert_eq!(next_maintenance(&rs, NOW).unwrap().name, "soon");
        assert_eq!(next_maintenance(&rs, NOW + 3600).unwrap().name, "later");
        assert_eq!(next_maintenance(&rs, NOW + 7200), None);
        assert_eq!(next_maintenance(&[], NOW), None);
        assert_eq!(next_maintenance(&[resv("x", NOW + 5, &[])], NOW), None);
    }

    #[test]
    fn fit_without_reservation_is_unchanged() {
        assert_eq!(fit(8 * 3600, NOW, None), Fit::Unchanged);
    }

    #[test]
    fn fit_ending_before_window_is_unchanged() {
        let r = resv("maint", NOW + 10 * 3600, &["MAINT"]);
        assert_eq!(fit(8 * 3600, NOW, Some(&r)), Fit::Unchanged);
        // Ending exactly at the start is allowed (job_end <= start).
        assert_eq!(fit(10 * 3600, NOW, Some(&r)), Fit::Unchanged);
    }

    #[test]
    fn fit_trims_to_margin_before_window() {
        let r = resv("maint-2026-09", NOW + 2 * 3600, &["MAINT"]);
        assert_eq!(
            fit(8 * 3600, NOW, Some(&r)),
            Fit::Trimmed {
                secs: 2 * 3600 - MAINT_MARGIN,
                reservation: "maint-2026-09".into(),
                ends_epoch: NOW + 2 * 3600 - MAINT_MARGIN,
            }
        );
        // One second over the start still trims.
        assert!(matches!(
            fit(2 * 3600 + 1, NOW, Some(&r)),
            Fit::Trimmed { .. }
        ));
    }

    #[test]
    fn fit_refuses_when_too_little_is_left() {
        // Exactly MIN after the margin is still allowed …
        let r = resv("maint", NOW + MAINT_MIN_SESSION + MAINT_MARGIN, &["MAINT"]);
        assert_eq!(
            fit(8 * 3600, NOW, Some(&r)),
            Fit::Trimmed {
                secs: MAINT_MIN_SESSION,
                reservation: "maint".into(),
                ends_epoch: NOW + MAINT_MIN_SESSION,
            }
        );
        // … one second less is refused.
        let r = resv(
            "maint",
            NOW + MAINT_MIN_SESSION + MAINT_MARGIN - 1,
            &["MAINT"],
        );
        assert_eq!(
            fit(8 * 3600, NOW, Some(&r)),
            Fit::Refuse {
                reservation: "maint".into(),
                starts_epoch: NOW + MAINT_MIN_SESSION + MAINT_MARGIN - 1,
            }
        );
        // A window that has already begun (the caller did not filter) refuses.
        let r = resv("maint", NOW - 5, &["MAINT"]);
        assert!(matches!(fit(60, NOW, Some(&r)), Fit::Refuse { .. }));
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn replace_time_forms() {
        let mut a = args(&["-p", "rna", "--time=8:00:00", "run.sh"]);
        assert!(replace_time_in_args(&mut a, "01:55:00"));
        assert_eq!(a, args(&["-p", "rna", "--time=01:55:00", "run.sh"]));

        let mut a = args(&["--time", "8:00:00", "run.sh"]);
        assert!(replace_time_in_args(&mut a, "01:55:00"));
        assert_eq!(a, args(&["--time", "01:55:00", "run.sh"]));

        let mut a = args(&["-t", "8:00:00", "run.sh"]);
        assert!(replace_time_in_args(&mut a, "01:55:00"));
        assert_eq!(a, args(&["-t", "01:55:00", "run.sh"]));

        let mut a = args(&["-t8:00:00", "run.sh"]);
        assert!(replace_time_in_args(&mut a, "01:55:00"));
        assert_eq!(a, args(&["-t01:55:00", "run.sh"]));

        let mut a = args(&["-t=8:00:00"]);
        assert!(replace_time_in_args(&mut a, "01:55:00"));
        assert_eq!(a, args(&["-t=01:55:00"]));
    }

    #[test]
    fn replace_time_rewrites_every_occurrence() {
        let mut a = args(&["-t", "1:00:00", "--time=2:00:00"]);
        assert!(replace_time_in_args(&mut a, "0:30:00"));
        assert_eq!(a, args(&["-t", "0:30:00", "--time=0:30:00"]));
    }

    #[test]
    fn replace_time_leaves_other_args_alone() {
        let mut a = args(&["--tmp=10G", "--test-only", "--time-min=5", "-N", "1"]);
        let before = a.clone();
        assert!(!replace_time_in_args(&mut a, "1:00:00"));
        assert_eq!(a, before);

        let mut a = args(&["-p", "rna", "run.sh"]);
        assert!(!replace_time_in_args(&mut a, "1:00:00"));
        assert_eq!(a, args(&["-p", "rna", "run.sh"]));

        // A trailing -t with no value has nothing to rewrite.
        let mut a = args(&["-t"]);
        assert!(!replace_time_in_args(&mut a, "1:00:00"));
        assert_eq!(a, args(&["-t"]));
    }
}
