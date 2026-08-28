//! `sinteractive cancel` — TODO(phase-1/agent-C).

use anyhow::Result;

use crate::cli::CancelArgs;

pub fn run(_args: CancelArgs) -> Result<i32> {
    anyhow::bail!("cancel is not implemented yet")
}
