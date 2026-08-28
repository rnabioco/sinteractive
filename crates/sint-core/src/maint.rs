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
pub fn next_maintenance(_reservations: &[Reservation], _now: i64) -> Option<Reservation> {
    // TODO(phase-1/agent-A)
    unimplemented!()
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

pub fn fit(_requested_secs: i64, _now: i64, _next: Option<&Reservation>) -> Fit {
    // TODO(phase-1/agent-A)
    unimplemented!()
}

/// Rewrite `--time`/`-t` in an sbatch arg list (`--time X`, `--time=X`, `-t X`, `-tX`).
pub fn replace_time_in_args(_args: &mut [String], _new_time: &str) -> bool {
    // TODO(phase-1/agent-A)
    unimplemented!()
}
