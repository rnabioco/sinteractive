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

/// Convert a human walltime to Slurm `[D-]HH:MM:SS` form.
pub fn parse_time(_input: &str) -> Result<String, String> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// Parse Slurm `D-HH:MM:SS`, `HH:MM:SS`, `MM:SS`, `MM`, `D-HH`, `D-HH:MM` to seconds.
pub fn slurm_time_to_seconds(_s: &str) -> Option<i64> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// Seconds to Slurm `[D-]HH:MM:SS`.
pub fn seconds_to_slurm_time(_secs: i64) -> String {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// `1d 2h 5m`, `3h 12m`, `45m`, or `Ns` under a minute (never `0m`).
pub fn format_short_duration(_secs: i64) -> String {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// Parse an `squeue %e`/`%S` timestamp (`2026-08-28T14:05:00`, local time) to
/// epoch seconds; `N/A`, `Unknown`, empty → `None`.
pub fn slurm_timestamp_to_epoch(_s: &str) -> Option<i64> {
    // TODO(phase-1/agent-A)
    unimplemented!()
}
