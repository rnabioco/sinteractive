//! Session notices: `<jobid>.notices`, TSV `kind\ttext`, one per line.
//!
//! Colour belongs to renderers; the file stays greppable. The file is
//! **removed** when there are no notices, so absence means "nothing to say".
//! Producers (script lines 1499-1600): quota overage, maintenance-trimmed end
//! time, and the Claude Code install hint (gated on a live `claude` process
//! and the integration not yet installed).

use serde::{Deserialize, Serialize};

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

/// Parse the TSV file contents.
pub fn parse_notices(_tsv: &str) -> Vec<Notice> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// Serialise to TSV (trailing newline; empty vec → empty string).
pub fn to_tsv(_notices: &[Notice]) -> String {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// Read notices for a job; missing file → empty.
pub fn read(_dir: &crate::state::StateDir, _job_id: u64) -> Vec<Notice> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// Write notices atomically; empty → remove the file.
pub fn write(
    _dir: &crate::state::StateDir,
    _job_id: u64,
    _notices: &[Notice],
) -> std::io::Result<()> {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// `QUOTA over by X (Y limit)` — the overage, because that is the number you
/// act on.
pub fn quota_notice(_over_kb: u64, _hard_kb: u64) -> Notice {
    // TODO(phase-1/agent-B): uses quota::kb_to_size
    unimplemented!()
}

/// `Session ends <date> — trimmed to finish before maintenance (<resv>)`.
pub fn maint_notice(_end_epoch: i64, _reservation: &str) -> Notice {
    // TODO(phase-1/agent-B)
    unimplemented!()
}

/// `Claude Code: run sinteractive install-claude to enable the skills and hooks`.
pub fn claude_hint_notice() -> Notice {
    Notice::new(
        "hint",
        "Claude Code: run sinteractive install-claude to enable the skills and hooks",
    )
}
