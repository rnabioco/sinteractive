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

/// Content rows the monitor panel draws below its accent rule: the job
/// strip, cpu, mem, and whatever GPU or history rows fit after them. The
/// pane itself is one row taller (see `layouts/sint-panel.kdl`).
pub const PANEL_ROWS: usize = 5;

/// The pane title the panel gives itself, so the bar can find it in the
/// pane manifest without guessing at plugin ids.
pub const PANEL_TITLE: &str = "sint-monitor";

/// What a bare keypress does while the panel holds the focus. The panel is
/// a selectable pane, so unbound keys arrive here instead of at the shell;
/// `Ctrl+b` chords never do — zellij resolves those as a mode switch before
/// the focused pane sees them, so every chord keeps working unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKey {
    Prev,
    Next,
    Top,
    Unfocus,
    Close,
}

impl PanelKey {
    /// Map a canonical key name (`main.rs` flattens zellij's
    /// `KeyWithModifier` into one) onto a panel action.
    pub fn from_name(name: &str) -> Option<PanelKey> {
        Some(match name {
            "left" | "h" | "," => PanelKey::Prev,
            "right" | "l" | "." => PanelKey::Next,
            "t" | "enter" => PanelKey::Top,
            "esc" | "q" => PanelKey::Unfocus,
            "x" => PanelKey::Close,
            _ => return None,
        })
    }
}

/// What the plugin must ask zellij for after an action. Keeping it an enum
/// leaves `State` free of zellij types, so every transition is testable
/// natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Effect {
    #[default]
    None,
    /// Insert the panel pane (the bar does this: no panel exists yet).
    OpenPanel,
    /// Give the panel pane the focus.
    FocusPanel,
    /// Hand the focus back to the shell, leaving the panel running.
    FocusShell,
    /// Close the panel pane.
    ClosePanel,
    /// Open the full `sinteractive monitor` TUI for the selected job in a
    /// floating pane.
    OpenTop,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub msg: StatusMsg,
    pub mode: BarMode,
    pub theme: ThemeMode,
    /// This instance is the panel (`view=monitor`), not the bar.
    pub is_panel: bool,
    /// The panel pane exists: the panel knows because it is it, the bar
    /// learns from the pane manifest.
    pub panel_open: bool,
    /// The panel pane holds the focus.
    pub focused: bool,
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

    /// A keybinding action, from the `sint-ui` pipe. Both instances see
    /// every action, so each answers only for the panes it owns.
    pub fn apply_action(&mut self, action: UiAction) -> Effect {
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
                Effect::None
            }
            UiAction::Help => {
                let pages = self.msg.help.len();
                self.mode = match self.mode {
                    BarMode::Help { page } if page + 1 < pages => BarMode::Help { page: page + 1 },
                    BarMode::Help { .. } => BarMode::Status,
                    _ if pages > 0 => BarMode::Help { page: 0 },
                    _ => BarMode::Status,
                };
                Effect::None
            }
            // `Ctrl+b m` is focus, not a toggle: it opens the panel when
            // none is running, moves the focus into it when one is, and
            // hands the focus back to the shell when the panel already has
            // it. The panel keeps running through all of that; `x` closes.
            UiAction::Monitor => match (self.is_panel, self.focused, self.panel_open) {
                (true, true, _) => Effect::FocusShell,
                (true, false, _) => Effect::FocusPanel,
                // The bar acts only when there is no panel instance to.
                (false, _, false) => Effect::OpenPanel,
                (false, _, true) => Effect::None,
            },
            UiAction::HostPrev => {
                let n = self.msg.hosts.len();
                if n > 0 {
                    self.host_idx = (self.host_idx + n - 1) % n;
                }
                Effect::None
            }
            UiAction::HostNext => {
                let n = self.msg.hosts.len();
                if n > 0 {
                    self.host_idx = (self.host_idx + 1) % n;
                }
                Effect::None
            }
            UiAction::Escape => {
                self.mode = BarMode::Status;
                Effect::None
            }
        }
    }

    /// A bare keypress in the focused panel.
    pub fn apply_key(&mut self, key: PanelKey) -> Effect {
        match key {
            PanelKey::Prev => self.apply_action(UiAction::HostPrev),
            PanelKey::Next => self.apply_action(UiAction::HostNext),
            PanelKey::Top => Effect::OpenTop,
            PanelKey::Unfocus => Effect::FocusShell,
            PanelKey::Close => Effect::ClosePanel,
        }
    }

    /// The job the panel is showing, if any.
    pub fn selected_job(&self) -> Option<u64> {
        self.msg.hosts.get(self.host_idx).map(|h| h.job_id)
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
            PANEL_ROWS + 1
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
        assert_eq!(s.apply_action(UiAction::Notices), Effect::None);
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
    fn ctrl_b_m_opens_then_moves_the_focus_back_and_forth() {
        // The bar opens the panel, and stands aside once one is running.
        let mut bar = State::default();
        assert_eq!(bar.apply_action(UiAction::Monitor), Effect::OpenPanel);
        bar.panel_open = true;
        assert_eq!(bar.apply_action(UiAction::Monitor), Effect::None);
        // The panel takes the focus, then gives it back — and stays open.
        let mut panel = State {
            is_panel: true,
            panel_open: true,
            ..Default::default()
        };
        assert_eq!(panel.apply_action(UiAction::Monitor), Effect::FocusPanel);
        panel.focused = true;
        assert_eq!(panel.apply_action(UiAction::Monitor), Effect::FocusShell);
        assert!(panel.panel_open);
    }

    #[test]
    fn panel_keys_select_jobs_and_leave() {
        let mut s = State {
            is_panel: true,
            panel_open: true,
            focused: true,
            ..Default::default()
        };
        s.apply_msg(msg_with(0, 3));
        for (name, want) in [
            ("right", PanelKey::Next),
            ("l", PanelKey::Next),
            ("left", PanelKey::Prev),
            ("h", PanelKey::Prev),
            ("t", PanelKey::Top),
            ("enter", PanelKey::Top),
            ("esc", PanelKey::Unfocus),
            ("q", PanelKey::Unfocus),
            ("x", PanelKey::Close),
        ] {
            assert_eq!(PanelKey::from_name(name), Some(want), "{name}");
        }
        assert_eq!(PanelKey::from_name("z"), None, "unbound keys pass through");
        assert_eq!(s.apply_key(PanelKey::Next), Effect::None);
        assert_eq!(s.selected_job(), Some(101));
        s.apply_key(PanelKey::Next);
        s.apply_key(PanelKey::Next);
        assert_eq!(s.selected_job(), Some(100), "wraps");
        s.apply_key(PanelKey::Prev);
        assert_eq!(s.selected_job(), Some(102));
        assert_eq!(s.apply_key(PanelKey::Top), Effect::OpenTop);
        assert_eq!(s.apply_key(PanelKey::Unfocus), Effect::FocusShell);
        assert_eq!(s.apply_key(PanelKey::Close), Effect::ClosePanel);
    }

    #[test]
    fn the_selector_follows_its_job_across_refreshes() {
        let mut s = State::default();
        s.apply_msg(msg_with(0, 3));
        // A refresh that drops hosts clamps the index; one that keeps the
        // selected job follows it.
        let mut m = msg_with(0, 2);
        m.hosts.reverse(); // job 101 now first
        s.host_idx = 2;
        s.apply_msg(m);
        assert_eq!(s.host_idx, 0);
        s.host_idx = 1; // job 100
        s.apply_msg(msg_with(0, 3)); // 100,101,102
        assert_eq!(s.host_idx, 0);
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
