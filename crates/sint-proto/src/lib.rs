//! Wire types shared by the node-side sampler (`sinteractive __job`) and the
//! zellij status plugin (`sint-zellij`).
//!
//! The sampler pushes a [`StatusMsg`] into the plugin about once a second via
//! `zellij pipe --name sint-status --plugin file:<wasm>`; the plugin is a pure
//! renderer of the latest message plus its own tiny UI state (which mode the
//! bar is in, which node the monitor panel shows). Keeping every number and
//! string on this side means the plugin never touches Slurm, the filesystem,
//! or NVML — all of that stays in native, testable code.
//!
//! This crate must compile for `wasm32-wasip1`: serde only, no I/O.

use serde::{Deserialize, Serialize};

/// Pipe name the sampler writes to and the keybindings send on.
pub const PIPE_NAME: &str = "sint-status";

/// Pipe name used by keybindings (`MessagePlugin`) to drive the bar's UI
/// state; the payload is one of the [`UiAction`] words.
pub const UI_PIPE_NAME: &str = "sint-ui";

/// How urgent the session line is; drives the glyph and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Plenty of walltime: steady `●`.
    #[default]
    Ok,
    /// Under the yellow threshold: blinking `●` + "Xh Ym left".
    Yellow,
    /// Under the red threshold: braille spinner + "M:SS left".
    Red,
    /// Walltime reached; the session is ending.
    Ending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub kind: String,
    pub text: String,
}

/// One GPU as shown in the monitor panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GpuLine {
    pub index: u32,
    pub name: String,
    /// 0–100.
    pub util_pct: u8,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub temp_c: Option<u32>,
    pub power_w: Option<u32>,
}

/// One process row in the monitor panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProcLine {
    pub pid: u32,
    pub user: String,
    pub cpu_pct: f32,
    pub rss_mb: u64,
    /// GPU memory in MB when the process holds a GPU context.
    pub gpu_mem_mb: Option<u64>,
    pub command: String,
}

/// A monitorable host: one of the user's RUNNING jobs' nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HostPanel {
    pub host: String,
    pub job_id: u64,
    pub job_name: Option<String>,
    /// Seconds since this snapshot was taken (staleness hint).
    pub age_secs: u64,
    pub cpu_pct: u8,
    pub cpu_alloc: u32,
    pub mem_used_mb: u64,
    pub mem_alloc_mb: u64,
    pub load1: f32,
    pub gpus: Vec<GpuLine>,
    pub procs: Vec<ProcLine>,
    /// Last 60 samples of CPU% for a sparkline, oldest first.
    pub cpu_history: Vec<u8>,
}

/// Which palette the bar and panel should draw with.
///
/// Zellij offers `HostTerminalThemeChanged`, but it is only as good as the
/// host terminal's answer to an OSC 11 that many terminals never send — and a
/// wrong `Light` there paints the light palette's dark grey (`#555555`) and
/// indigo (`#5769F7`) onto a dark background, where they read as barely-there
/// text. The session resolves the mode itself (`SINTERACTIVE_THEME`, then
/// `COLORFGBG`, then dark) and says so here; the plugin trusts this over the
/// event whenever it is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePref {
    Dark,
    Light,
}

/// The whole message. Every field is a plain value the plugin can print.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StatusMsg {
    pub job_id: u64,
    pub name: Option<String>,
    pub host: String,
    pub severity: Severity,
    /// `3h 12m`, `9:41`, or empty when the deadline is unknown.
    pub remaining: String,
    /// Seconds remaining, for the plugin's own countdown between messages.
    pub remaining_secs: Option<i64>,
    /// Local host load, rendered as `cpu 34% 12/32G`; empty until phase 3.
    pub load: String,
    /// `gpu0 87% 31/40G`; empty when there are no GPUs.
    pub gpu: String,
    /// Jobs launched from this session: `2R 1PD`; empty when none — which
    /// is most of the time, and says nothing worth a segment.
    pub jobs: String,
    pub notices: Vec<Notice>,
    /// Key legend pages for help mode: `[[("n", "notices"), …], …]`.
    pub help: Vec<Vec<(String, String)>>,
    /// Monitorable hosts for the panel, in selector order.
    pub hosts: Vec<HostPanel>,
    /// Sampler-side timestamp (epoch seconds), so the plugin can show staleness.
    pub sent_epoch: i64,
    /// The palette the session resolved; `None` leaves it to zellij's event.
    pub theme: Option<ThemePref>,
}

/// Keybinding-driven UI actions, sent as the payload on [`UI_PIPE_NAME`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiAction {
    /// Toggle notices mode / advance to the next notice.
    Notices,
    /// Toggle help mode / next page.
    Help,
    /// Toggle the monitor panel.
    Monitor,
    /// Previous host in the monitor selector.
    HostPrev,
    /// Next host in the monitor selector.
    HostNext,
    /// Leave any mode; back to the status line.
    Escape,
}

impl UiAction {
    /// The inverse of [`UiAction::as_str`]. A plain match, not a serde
    /// round trip: this is the plugin's keypress path, and every byte of
    /// deserializer it does not pull in is a byte the zellij server does
    /// not load.
    pub fn parse(s: &str) -> Option<UiAction> {
        Some(match s.trim() {
            "notices" => UiAction::Notices,
            "help" => UiAction::Help,
            "monitor" => UiAction::Monitor,
            "host-prev" => UiAction::HostPrev,
            "host-next" => UiAction::HostNext,
            "escape" => UiAction::Escape,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            UiAction::Notices => "notices",
            UiAction::Help => "help",
            UiAction::Monitor => "monitor",
            UiAction::HostPrev => "host-prev",
            UiAction::HostNext => "host-next",
            UiAction::Escape => "escape",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_action_round_trips() {
        for a in [
            UiAction::Notices,
            UiAction::Help,
            UiAction::Monitor,
            UiAction::HostPrev,
            UiAction::HostNext,
            UiAction::Escape,
        ] {
            assert_eq!(UiAction::parse(a.as_str()), Some(a));
        }
        assert_eq!(UiAction::parse("bogus"), None);
    }

    #[test]
    fn status_msg_json_round_trip() {
        let m = StatusMsg {
            job_id: 1,
            host: "n1".into(),
            severity: Severity::Yellow,
            remaining: "42m".into(),
            notices: vec![Notice {
                kind: "quota".into(),
                text: "over".into(),
            }],
            ..Default::default()
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"severity\":\"yellow\""));
        assert_eq!(serde_json::from_str::<StatusMsg>(&s).unwrap(), m);
    }
}
