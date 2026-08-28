//! zellij glue: events in, `render` out.
//!
//! zellij plugins are WASI *commands* (the server calls `_start`, then the
//! exported `load`/`update`/`pipe`/`render`), so this crate's binary is the
//! plugin and `register_plugin!` — which also defines `main` — lives here at
//! the crate root. Natively the binary is an empty stub; the logic in the
//! library (`state`, `render`) is what the tests exercise.
//!
//! The same `.wasm` runs as two instances, told apart by the `view`
//! configuration key the layout passes:
//!
//! - `view=bar` (default): the one-row status line at the bottom.
//! - `view=monitor`: the monitor panel, a fixed-height pane just above the
//!   bar that hides itself on load and shows itself on `Ctrl+b m`. zellij
//!   refuses to resize fixed-size panes, so the panel is a separate pane that
//!   is suppressed/unsuppressed rather than the bar growing.
//!
//! Both instances receive every `sint-status`/`sint-ui` pipe (the sender
//! broadcasts rather than targeting a plugin url), so they share one
//! `State` transition table and stay in sync.

#[cfg(not(target_arch = "wasm32"))]
fn main() {}

#[cfg(target_arch = "wasm32")]
use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
use zellij_tile::prelude::*;

#[cfg(target_arch = "wasm32")]
use sint_proto::{StatusMsg, UiAction, PIPE_NAME, UI_PIPE_NAME};
#[cfg(target_arch = "wasm32")]
use sint_zellij::render;
#[cfg(target_arch = "wasm32")]
use sint_zellij::state::{State, ThemeMode};

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct Plugin {
    st: State,
    /// True for the monitor-panel instance.
    is_panel: bool,
}

#[cfg(target_arch = "wasm32")]
register_plugin!(Plugin);

#[cfg(target_arch = "wasm32")]
const TICK_SECS: f64 = 0.5;

#[cfg(target_arch = "wasm32")]
impl ZellijPlugin for Plugin {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_panel = configuration.get("view").map(String::as_str) == Some("monitor");
        // The panel instance only exists while the panel is open; it is
        // created by the layout swap that opens it, so it starts "open" and
        // the next toggle closes it.
        self.st.panel_open = self.is_panel;
        // Subscribe before asking: with the permission pre-granted in
        // zellij's cache the answer arrives immediately, and an event sent
        // before the subscription is dropped.
        subscribe(&[
            EventType::Timer,
            EventType::PermissionRequestResult,
            EventType::HostTerminalThemeChanged,
        ]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        set_selectable(false);
        set_timeout(TICK_SECS);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Timer(_) => {
                set_timeout(TICK_SECS);
                self.st.tick()
            }
            Event::PermissionRequestResult(status) => {
                eprintln!("sint-zellij: permission {:?}", status);
                true
            }
            Event::HostTerminalThemeChanged(mode) => {
                // Only until the session says otherwise: this is the host
                // terminal's answer to an OSC 11 it may never have sent, and a
                // wrong `Light` paints the light palette's dark grey and
                // indigo onto a dark background.
                if self.st.theme_from_session {
                    return false;
                }
                self.st.theme = match mode {
                    HostTerminalThemeMode::Light => ThemeMode::Light,
                    _ => ThemeMode::Dark,
                };
                true
            }
            _ => false,
        }
    }

    fn pipe(&mut self, msg: PipeMessage) -> bool {
        match msg.name.as_str() {
            PIPE_NAME => {
                if let Some(payload) = msg.payload {
                    match serde_json::from_str::<StatusMsg>(&payload) {
                        Ok(m) => self.st.apply_msg(m),
                        Err(e) => eprintln!("sint-zellij: bad status message: {e}"),
                    }
                }
                true
            }
            UI_PIPE_NAME => {
                let action = msg
                    .payload
                    .as_deref()
                    .and_then(UiAction::parse)
                    .or_else(|| msg.args.get("action").and_then(|a| UiAction::parse(a)));
                if let Some(a) = action {
                    let toggled = self.st.apply_action(a);
                    if toggled {
                        self.sync_panel();
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.st.rows = rows;
        self.st.cols = cols;
        if self.is_panel {
            print!("{}", render::render_panel(&self.st, rows, cols));
        } else {
            print!("{}", render::render(&self.st, rows, cols));
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Plugin {
    /// Open or close the panel pane after a toggle.
    ///
    /// Opening: the *bar* re-applies the tab layout that includes the panel
    /// (`sint-panel`), retaining the terminal panes and itself; zellij
    /// creates the panel instance in the layout's slot, between the shell
    /// and the bar. Closing: the *panel* closes itself, and its rows go back
    /// to the shell. (A layout swap on close would keep the retained panel
    /// instance alive and re-insert it beside the shell.)
    fn sync_panel(&mut self) {
        if self.st.panel_open && !self.is_panel {
            override_layout(
                LayoutInfo::File("sint-panel".to_string(), LayoutMetadata::default()),
                true,
                true,
                true,
                BTreeMap::new(),
            );
        } else if !self.st.panel_open && self.is_panel {
            close_self();
        }
    }
}
