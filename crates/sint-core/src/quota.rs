//! Storage quota (Bodhi).
//!
//! Ports script lines 1252-1476. Bodhi's `quota_check` lives only on the head
//! node, but its daemons answer compute nodes directly and the hard-limit
//! file is on shared storage, so a session can probe from where it runs.
//!
//! - hard limit: awk over `user|size|email` lines in `SINTERACTIVE_QUOTA_FILE`
//!   (read **first** — local, and it gates the network half)
//! - usage: for each host, TCP connect, send `QUOTA <uid>\n`, read `OK <kb>`,
//!   sum. Down daemons are skipped; **zero answers is a hard failure** (a
//!   partial sum could silently clear a real warning). Connect/read timeouts
//!   are real here, unlike bash `/dev/tcp`.
//! - cache: `quota.json`, per user (not per job), so per-job teardown leaves it.
//!
//! On clusters without the daemons (Alpine) every call reports "unavailable"
//! and no notice is ever produced.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuotaSnapshot {
    pub user: String,
    pub used_kb: u64,
    pub hard_kb: u64,
    pub over_kb: u64,
    /// Percent used, integer.
    pub pct: u64,
    pub over: bool,
    pub checked_epoch: i64,
}

/// `500G` → KiB; IEC ladder (`K`,`M`,`G`,`T`,`P`), bare number = KiB.
pub fn size_to_kb(_s: &str) -> Option<u64> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// KiB → human (`12.3G`, `500G`, `1.2T`), mirroring `quota_check`'s output.
pub fn kb_to_size(_kb: u64) -> String {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// Hard limit for `user` from the quota file contents.
pub fn parse_hard_kb(_file_contents: &str, _user: &str) -> Option<u64> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// Probe now. `Err` when the file has no entry, or no daemon answered.
pub fn probe(
    _cfg: &crate::config::Config,
    _user: &str,
    _uid: u32,
) -> anyhow::Result<QuotaSnapshot> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// Read the cache.
pub fn cached(_dir: &crate::state::StateDir) -> Option<QuotaSnapshot> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

pub fn write_cache(_dir: &crate::state::StateDir, _q: &QuotaSnapshot) -> std::io::Result<()> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}
