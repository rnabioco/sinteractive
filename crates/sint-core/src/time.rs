//! Walltime parsing and formatting.
//!
//! Ports `parse_time` (script line 79), `slurm_time_to_seconds` (1922),
//! `seconds_to_slurm_time` (1876) and `format_short_duration` (1953).
//!
//! Accepted human forms: `8h`, `30m`, `2d`, `1d12h`, `1h30m`, `90m`. A bare
//! integer is minutes (Slurm native). Anything containing `:` or of the form
//! `N-…` is passed through as already-Slurm. Carries are normalised
//! (`90m` → `1:30:00`, `25h` → `1-01:00:00`). Unrecognised input is returned
//! unchanged with `Err` carrying a warning the caller may print.

use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

/// Convert a human walltime to Slurm `[D-]HH:MM:SS` form.
///
/// `Err` carries the warning the bash printed ("unrecognized time format
/// '…', passing through to SLURM"); the caller is expected to print it and
/// still hand `input` to sbatch unchanged, which is what the bash did.
pub fn parse_time(input: &str) -> Result<String, String> {
    // Already in Slurm format (contains ':' or starts with 'D-'): pass through.
    if input.contains(':') || starts_with_days_dash(input) {
        return Ok(input.to_string());
    }

    // Pure integer = minutes (Slurm native).
    if !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(input.to_string());
    }

    let (mut days, mut hours, mut minutes) = (0u64, 0u64, 0u64);
    let mut rest = input;
    if let Some((n, r)) = take_unit(rest, 'd') {
        days = n;
        rest = r;
    }
    if let Some((n, r)) = take_unit(rest, 'h') {
        hours = n;
        rest = r;
    }
    if let Some((n, r)) = take_unit(rest, 'm') {
        minutes = n;
        rest = r;
    }

    // Anything left over (including an empty input) is not a form we know.
    if !rest.is_empty() || input.is_empty() {
        return Err(format!(
            "unrecognized time format '{input}', passing through to SLURM"
        ));
    }

    // Normalise carries so e.g. 90m -> 1h30m and 25h -> 1d1h, keeping each
    // field within range for Slurm's [D-]HH:MM:SS format.
    hours += minutes / 60;
    minutes %= 60;
    days += hours / 24;
    hours %= 24;

    if days > 0 {
        Ok(format!("{days}-{hours:02}:{minutes:02}:00"))
    } else {
        Ok(format!("{hours:02}:{minutes:02}:00"))
    }
}

/// `^[0-9]+-` — the bash test for an already-Slurm `D-…` form.
fn starts_with_days_dash(s: &str) -> bool {
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    digits > 0 && s.as_bytes().get(digits) == Some(&b'-')
}

/// Match `^([0-9]+)UNIT` at the front of `s`; returns the number and the
/// remainder after the unit letter.
fn take_unit(s: &str, unit: char) -> Option<(u64, &str)> {
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = s[digits..].strip_prefix(unit)?;
    let n = s[..digits].parse().ok()?;
    Some((n, rest))
}

/// Parse Slurm `D-HH:MM:SS`, `HH:MM:SS`, `MM:SS`, `MM`, `D-HH`, `D-HH:MM` to seconds.
pub fn slurm_time_to_seconds(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let num = |p: &str| -> Option<i64> {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };

    let (days, clock, has_days) = match s.split_once('-') {
        Some((d, rest)) => (num(d)?, rest, true),
        None => (0, s, false),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let (h, m, sec) = if has_days {
        // D-HH, D-HH:MM, D-HH:MM:SS
        match parts.as_slice() {
            [h] => (num(h)?, 0, 0),
            [h, m] => (num(h)?, num(m)?, 0),
            [h, m, sec] => (num(h)?, num(m)?, num(sec)?),
            _ => return None,
        }
    } else {
        // MM, MM:SS, HH:MM:SS
        match parts.as_slice() {
            [m] => (0, num(m)?, 0),
            [m, sec] => (0, num(m)?, num(sec)?),
            [h, m, sec] => (num(h)?, num(m)?, num(sec)?),
            _ => return None,
        }
    };
    Some(days * 86_400 + h * 3_600 + m * 60 + sec)
}

/// Seconds to Slurm `[D-]HH:MM:SS`.
pub fn seconds_to_slurm_time(secs: i64) -> String {
    let mut s = secs.max(0);
    let d = s / 86_400;
    s %= 86_400;
    let h = s / 3_600;
    let m = (s % 3_600) / 60;
    s %= 60;
    if d > 0 {
        format!("{d}-{h:02}:{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

/// `1d 2h 5m`, `3h 12m`, `45m`, or `Ns` under a minute (never `0m`).
pub fn format_short_duration(secs: i64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    let mut out = String::new();
    if d > 0 {
        out.push_str(&format!("{d}d "));
    }
    if h > 0 {
        out.push_str(&format!("{h}h "));
    }
    if m > 0 {
        out.push_str(&format!("{m}m"));
    }
    // Below one minute, minute granularity reads as a confusing "0m".
    if d == 0 && h == 0 && m == 0 {
        out = format!("{secs}s");
    }
    out.trim_end().to_string()
}

const TIMESTAMP_FMT: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");

/// Parse an `squeue %e`/`%S` timestamp (`2026-08-28T14:05:00`, local time) to
/// epoch seconds; `N/A`, `Unknown`, empty → `None`.
pub fn slurm_timestamp_to_epoch(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let naive = PrimitiveDateTime::parse(s, TIMESTAMP_FMT).ok()?;
    Some(local_naive_to_epoch(naive))
}

/// Interpret a wall-clock time as local time, the way `date -d` did.
///
/// The offset depends on the instant (DST), and the instant depends on the
/// offset; two passes settle it for every time except the ambiguous hour at
/// a fall-back transition, where either answer is defensible. If the local
/// offset cannot be determined the time is taken as UTC.
fn local_naive_to_epoch(naive: PrimitiveDateTime) -> i64 {
    let guess = naive.assume_utc();
    let off1 = local_offset_at(guess);
    let first = naive.assume_offset(off1);
    let off2 = local_offset_at(first);
    if off2 == off1 {
        first.unix_timestamp()
    } else {
        naive.assume_offset(off2).unix_timestamp()
    }
}

/// The system's UTC offset at `at`, or UTC when it cannot be determined
/// (older `time` releases refuse the query in a multithreaded process).
fn local_offset_at(at: OffsetDateTime) -> UtcOffset {
    UtcOffset::local_offset_at(at).unwrap_or(UtcOffset::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_human_forms() {
        assert_eq!(parse_time("8h").unwrap(), "08:00:00");
        assert_eq!(parse_time("30m").unwrap(), "00:30:00");
        assert_eq!(parse_time("2d").unwrap(), "2-00:00:00");
        assert_eq!(parse_time("1d12h").unwrap(), "1-12:00:00");
        assert_eq!(parse_time("1h30m").unwrap(), "01:30:00");
        assert_eq!(parse_time("1d2h5m").unwrap(), "1-02:05:00");
    }

    #[test]
    fn parse_time_normalises_carries() {
        assert_eq!(parse_time("90m").unwrap(), "01:30:00");
        assert_eq!(parse_time("25h").unwrap(), "1-01:00:00");
        assert_eq!(parse_time("1500m").unwrap(), "1-01:00:00");
        assert_eq!(parse_time("48h").unwrap(), "2-00:00:00");
    }

    #[test]
    fn parse_time_bare_integer_is_minutes() {
        assert_eq!(parse_time("30").unwrap(), "30");
        assert_eq!(parse_time("120").unwrap(), "120");
    }

    #[test]
    fn parse_time_passes_slurm_forms_through() {
        assert_eq!(parse_time("2-00:00:00").unwrap(), "2-00:00:00");
        assert_eq!(parse_time("08:00:00").unwrap(), "08:00:00");
        assert_eq!(parse_time("8:00:00").unwrap(), "8:00:00");
        assert_eq!(parse_time("1-12").unwrap(), "1-12");
        assert_eq!(parse_time("30:00").unwrap(), "30:00");
    }

    #[test]
    fn parse_time_rejects_unknown() {
        for bad in ["8x", "abc", "h8", "8hh", "1m30s", "", "8 h", "-1"] {
            let err = parse_time(bad).unwrap_err();
            assert!(err.contains("unrecognized time format"), "{bad}: {err}");
            assert!(err.contains(&format!("'{bad}'")), "{bad}: {err}");
        }
    }

    #[test]
    fn parse_time_unit_order_is_fixed() {
        // The bash only accepts d, then h, then m; "30m1h" leaves "1h" over.
        assert!(parse_time("30m1h").is_err());
        assert!(parse_time("2h1d").is_err());
    }

    #[test]
    fn slurm_time_to_seconds_forms() {
        assert_eq!(slurm_time_to_seconds("1-02:03:04"), Some(93_784));
        assert_eq!(slurm_time_to_seconds("1-02:03"), Some(93_780));
        assert_eq!(slurm_time_to_seconds("1-02"), Some(93_600));
        assert_eq!(slurm_time_to_seconds("02:03:04"), Some(7_384));
        assert_eq!(slurm_time_to_seconds("8:00:00"), Some(28_800));
        assert_eq!(slurm_time_to_seconds("03:04"), Some(184));
        assert_eq!(slurm_time_to_seconds("120"), Some(7_200));
        assert_eq!(slurm_time_to_seconds("0-00:10"), Some(600));
        assert_eq!(slurm_time_to_seconds("08:05:09"), Some(29_109));
    }

    #[test]
    fn slurm_time_to_seconds_rejects_garbage() {
        for bad in [
            "",
            "UNLIMITED",
            "N/A",
            "8h",
            "1-",
            "-1",
            "1:2:3:4",
            "1-2-3",
            "a:b",
            "1:",
        ] {
            assert_eq!(slurm_time_to_seconds(bad), None, "{bad}");
        }
    }

    #[test]
    fn seconds_to_slurm_time_roundtrips() {
        assert_eq!(seconds_to_slurm_time(0), "00:00:00");
        assert_eq!(seconds_to_slurm_time(-5), "00:00:00");
        assert_eq!(seconds_to_slurm_time(59), "00:00:59");
        assert_eq!(seconds_to_slurm_time(28_800), "08:00:00");
        assert_eq!(seconds_to_slurm_time(93_784), "1-02:03:04");
        assert_eq!(seconds_to_slurm_time(86_400), "1-00:00:00");
        for s in [1, 61, 3_601, 86_399, 86_401, 200_000] {
            assert_eq!(slurm_time_to_seconds(&seconds_to_slurm_time(s)), Some(s));
        }
    }

    #[test]
    fn format_short_duration_forms() {
        assert_eq!(format_short_duration(0), "0s");
        assert_eq!(format_short_duration(45), "45s");
        assert_eq!(format_short_duration(60), "1m");
        assert_eq!(format_short_duration(2_700), "45m");
        assert_eq!(format_short_duration(3_600), "1h");
        assert_eq!(format_short_duration(3 * 3_600 + 12 * 60 + 30), "3h 12m");
        assert_eq!(
            format_short_duration(86_400 + 2 * 3_600 + 5 * 60),
            "1d 2h 5m"
        );
        assert_eq!(format_short_duration(86_400), "1d");
        assert_eq!(format_short_duration(86_400 + 5 * 60), "1d 5m");
        assert_eq!(format_short_duration(600), "10m");
    }

    #[test]
    fn timestamp_missing_values_are_none() {
        for s in ["", "N/A", "Unknown", "UNLIMITED", "(null)", "   "] {
            assert_eq!(slurm_timestamp_to_epoch(s), None, "{s:?}");
        }
    }

    #[test]
    fn timestamp_parses_as_local_time() {
        let s = "2026-08-28T14:05:00";
        let epoch = slurm_timestamp_to_epoch(s).expect("parses");
        // Re-derive what the instant looks like in local time and compare
        // wall-clock fields; this holds in every zone (UTC fallback included).
        let instant = OffsetDateTime::from_unix_timestamp(epoch).unwrap();
        let off = local_offset_at(instant);
        let local = instant.to_offset(off);
        assert_eq!(local.year(), 2026);
        assert_eq!(u8::from(local.month()), 8);
        assert_eq!(local.day(), 28);
        assert_eq!(local.hour(), 14);
        assert_eq!(local.minute(), 5);
        assert_eq!(local.second(), 0);
        // And the naive-as-UTC reading differs by exactly the local offset.
        let as_utc = PrimitiveDateTime::parse(s, TIMESTAMP_FMT)
            .unwrap()
            .assume_utc()
            .unix_timestamp();
        assert_eq!(as_utc - epoch, i64::from(off.whole_seconds()));
    }

    #[test]
    fn timestamp_ordering_is_preserved() {
        let a = slurm_timestamp_to_epoch("2026-01-01T00:00:00").unwrap();
        let b = slurm_timestamp_to_epoch("2026-01-01T00:00:01").unwrap();
        let c = slurm_timestamp_to_epoch("2026-01-02T00:00:00").unwrap();
        assert_eq!(b - a, 1);
        assert_eq!(c - a, 86_400);
    }

    #[test]
    fn timestamp_rejects_partial_forms() {
        assert_eq!(slurm_timestamp_to_epoch("2026-08-28"), None);
        assert_eq!(slurm_timestamp_to_epoch("2026-08-28T14:05"), None);
        assert_eq!(slurm_timestamp_to_epoch("2026-13-01T00:00:00"), None);
    }
}
