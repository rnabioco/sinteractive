//! `sinteractive ensure NAME` — TODO(phase-1/agent-C).

use anyhow::Result;

use crate::cli::EnsureArgs;

pub fn run(_args: EnsureArgs) -> Result<i32> {
    anyhow::bail!("ensure is not implemented yet")
}
