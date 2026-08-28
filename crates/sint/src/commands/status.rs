//! `sinteractive status` / `refresh` — TODO(phase-1/agent-C).

use anyhow::Result;

use crate::cli::TargetArgs;

/// `refresh` = true pokes the session's cache before reporting.
pub fn run(_args: TargetArgs, _refresh: bool) -> Result<i32> {
    anyhow::bail!("status is not implemented yet")
}
