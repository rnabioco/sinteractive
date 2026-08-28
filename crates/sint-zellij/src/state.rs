//! Plugin UI state and its transitions — no zellij types here.

use sint_proto::{StatusMsg, UiAction};

/// Which content the bar line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarMode {
    #[default]
    Status,
    /// Showing notice `idx` of N.
    Notices { idx: usize },
    /// Showing help page `page`.
    Help { page: usize },
}

/// Terminal background, from zellij's `HostTerminalThemeChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Rows the pane grows to when the monitor panel is on (status line + panel).
pub const PANEL_ROWS: usize = 12;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub msg: StatusMsg,
    pub mode: BarMode,
    pub theme: ThemeMode,
    pub panel_open: bool,
    /// Index into `msg.hosts` shown by the panel.
    pub host_idx: usize,
    /// Animation frame counter (advanced by the timer).
    pub frame: u64,
    /// Seconds since the last `StatusMsg` (for staleness and countdown).
    pub since_msg_secs: u64,
    /// Ticks left before an auto-cycle in notices mode / auto-return.
    pub mode_ttl: u32,
    pub cols: usize,
    pub rows: usize,
}

/// Auto-advance notices every N ticks (ticks are ~0.5 s).
pub const NOTICE_CYCLE_TICKS: u32 = 8;
/// Return to status mode after N idle ticks in help/notices mode.
pub const MODE_IDLE_TICKS: u32 = 40;

impl State {
    pub fn apply_msg(&mut self, msg: StatusMsg) {
        // Keep the selector stable across refreshes: follow the same job id.
        if let Some(cur) = self.msg.hosts.get(self.host_idx) {
            if let Some(i) = msg.hosts.iter().position(|h| h.job_id == cur.job_id) {
                self.host_idx = i;
            }
        }
        if self.host_idx >= msg.hosts.len() {
            self.host_idx = 0;
        }
        if let BarMode::Notices { idx } = self.mode {
            if idx >= msg.notices.len() {
                self.mode = BarMode::Status;
            }
        }
        self.msg = msg;
        self.since_msg_secs = 0;
    }

    /// A keybinding action. Returns true when the pane height should change
    /// (panel toggled), so the caller can resize.
    pub fn apply_action(&mut self, action: UiAction) -> bool {
        self.mode_ttl = MODE_IDLE_TICKS;
        match action {
            UiAction::Notices => {
                let n = self.msg.notices.len();
                self.mode = match self.mode {
                    BarMode::Notices { idx } if idx + 1 < n => BarMode::Notices { idx: idx + 1 },
                    BarMode::Notices { .. } => BarMode::Status,
                    _ if n > 0 => BarMode::Notices { idx: 0 },
                    _ => BarMode::Status,
                };
                false
            }
            UiAction::Help => {
                let pages = self.msg.help.len();
                self.mode = match self.mode {
                    BarMode::Help { page } if page + 1 < pages => BarMode::Help { page: page + 1 },
                    BarMode::Help { .. } => BarMode::Status,
                    _ if pages > 0 => BarMode::Help { page: 0 },
                    _ => BarMode::Status,
                };
                false
            }
            UiAction::Monitor => {
                self.panel_open = !self.panel_open;
                true
            }
            UiAction::HostPrev => {
                let n = self.msg.hosts.len();
                if n > 0 {
                    self.host_idx = (self.host_idx + n - 1) % n;
                }
                false
            }
            UiAction::HostNext => {
                let n = self.msg.hosts.len();
                if n > 0 {
                    self.host_idx = (self.host_idx + 1) % n;
                }
                false
            }
            UiAction::Escape => {
                self.mode = BarMode::Status;
                false
            }
        }
    }

    /// A timer tick (~0.5 s). Returns true when a redraw is needed.
    pub fn tick(&mut self) -> bool {
        self.frame = self.frame.wrapping_add(1);
        if self.frame % 2 == 0 {
            self.since_msg_secs += 1;
        }
        match self.mode {
            BarMode::Status => {}
            BarMode::Notices { idx } => {
                if self.mode_ttl > 0 {
                    self.mode_ttl -= 1;
                }
                if self.mode_ttl == 0 {
                    self.mode = BarMode::Status;
                } else if self.frame % NOTICE_CYCLE_TICKS as u64 == 0 && self.msg.notices.len() > 1
                {
                    self.mode = BarMode::Notices {
                        idx: (idx + 1) % self.msg.notices.len(),
                    };
                }
            }
            BarMode::Help { .. } => {
                if self.mode_ttl > 0 {
                    self.mode_ttl -= 1;
                }
                if self.mode_ttl == 0 {
                    self.mode = BarMode::Status;
                }
            }
        }
        true
    }

    /// Height the pane should have right now.
    pub fn wanted_rows(&self) -> usize {
        if self.panel_open {
            PANEL_ROWS
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sint_proto::{HostPanel, Notice};

    fn msg_with(n_notices: usize, n_hosts: usize) -> StatusMsg {
        StatusMsg {
            notices: (0..n_notices)
                .map(|i| Notice {
                    kind: "hint".into(),
                    text: format!("n{i}"),
                })
                .collect(),
            help: vec![vec![("n".into(), "notices".into())], vec![]],
            hosts: (0..n_hosts)
                .map(|i| HostPanel {
                    host: format!("h{i}"),
                    job_id: 100 + i as u64,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn notices_cycle_then_return() {
        let mut s = State::default();
        s.apply_msg(msg_with(2, 0));
        assert!(!s.apply_action(UiAction::Notices));
        assert_eq!(s.mode, BarMode::Notices { idx: 0 });
        s.apply_action(UiAction::Notices);
        assert_eq!(s.mode, BarMode::Notices { idx: 1 });
        s.apply_action(UiAction::Notices);
        assert_eq!(s.mode, BarMode::Status);
        // No notices: stays in status mode.
        s.apply_msg(msg_with(0, 0));
        s.apply_action(UiAction::Notices);
        assert_eq!(s.mode, BarMode::Status);
    }

    #[test]
    fn help_pages_and_escape() {
        let mut s = State::default();
        s.apply_msg(msg_with(0, 0));
        s.apply_action(UiAction::Help);
        assert_eq!(s.mode, BarMode::Help { page: 0 });
        s.apply_action(UiAction::Help);
        assert_eq!(s.mode, BarMode::Help { page: 1 });
        s.apply_action(UiAction::Escape);
        assert_eq!(s.mode, BarMode::Status);
    }

    #[test]
    fn panel_toggle_and_host_selector_wrap() {
        let mut s = State::default();
        s.apply_msg(msg_with(0, 3));
        assert!(s.apply_action(UiAction::Monitor));
        assert!(s.panel_open);
        assert_eq!(s.wanted_rows(), PANEL_ROWS);
        s.apply_action(UiAction::HostNext);
        s.apply_action(UiAction::HostNext);
        s.apply_action(UiAction::HostNext);
        assert_eq!(s.host_idx, 0);
        s.apply_action(UiAction::HostPrev);
        assert_eq!(s.host_idx, 2);
        // A refresh that drops hosts clamps the index; one that keeps the
        // selected job follows it.
        let mut m = msg_with(0, 2);
        m.hosts.reverse(); // job 101 now first
        s.host_idx = 2;
        s.apply_msg(m);
        assert_eq!(s.host_idx, 0);
        s.host_idx = 1; // job 100
        let m2 = msg_with(0, 3); // 100,101,102
        s.apply_msg(m2);
        assert_eq!(s.host_idx, 0);
        assert!(s.apply_action(UiAction::Monitor));
        assert_eq!(s.wanted_rows(), 1);
    }

    #[test]
    fn idle_returns_to_status() {
        let mut s = State::default();
        s.apply_msg(msg_with(1, 0));
        s.apply_action(UiAction::Notices);
        for _ in 0..MODE_IDLE_TICKS {
            s.tick();
        }
        assert_eq!(s.mode, BarMode::Status);
    }
}
