//! `sinteractive quota` — TODO(phase-1/agent-C).

use anyhow::Result;

use crate::cli::QuotaArgs;

pub fn run(_args: QuotaArgs) -> Result<i32> {
    anyhow::bail!("quota is not implemented yet")
}
