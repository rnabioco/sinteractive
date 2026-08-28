//! zellij glue (wasm only): events in, `render` out.

use std::collections::BTreeMap;

use zellij_tile::prelude::*;

use crate::render;
use crate::state::{State, ThemeMode};
use sint_proto::{StatusMsg, UiAction, PIPE_NAME, UI_PIPE_NAME};

#[derive(Default)]
struct Plugin {
    st: State,
    own_pane: Option<PaneId>,
    permitted: bool,
    /// Rows the pane currently has, from the last render call.
    rows: usize,
}

register_plugin!(Plugin);

const TICK_SECS: f64 = 0.5;

impl ZellijPlugin for Plugin {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[
            EventType::Timer,
            EventType::PermissionRequestResult,
            EventType::HostTerminalThemeChanged,
            EventType::ModeUpdate,
        ]);
        set_selectable(false);
        let ids = get_plugin_ids();
        self.own_pane = Some(PaneId::Plugin(ids.plugin_id));
        set_timeout(TICK_SECS);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Timer(_) => {
                set_timeout(TICK_SECS);
                self.st.tick()
            }
            Event::PermissionRequestResult(status) => {
                self.permitted = matches!(status, PermissionStatus::Granted);
                self.fit_height();
                true
            }
            Event::HostTerminalThemeChanged(mode) => {
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
                    if let Ok(m) = serde_json::from_str::<StatusMsg>(&payload) {
                        self.st.apply_msg(m);
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
                    if self.st.apply_action(a) {
                        self.fit_height();
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.rows = rows;
        self.st.rows = rows;
        self.st.cols = cols;
        print!("{}", render::render(&self.st, rows, cols));
    }
}

impl Plugin {
    /// Grow or shrink our own pane towards `wanted_rows`. zellij resizes in
    /// fixed steps, so this nudges once per call and the next render's `rows`
    /// tells us whether to keep going (the timer tick re-invokes it).
    fn fit_height(&mut self) {
        if !self.permitted {
            return;
        }
        let Some(pane) = self.own_pane else { return };
        let want = self.st.wanted_rows();
        if self.rows == 0 || self.rows == want {
            return;
        }
        let strategy = if self.rows < want {
            ResizeStrategy::new(Resize::Increase, Some(Direction::Up))
        } else {
            ResizeStrategy::new(Resize::Decrease, Some(Direction::Up))
        };
        resize_pane_with_id(strategy, pane);
    }
}
