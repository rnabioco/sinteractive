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
//!   bar. zellij refuses to resize fixed-size panes, so the panel is a
//!   separate pane that is inserted by a layout swap rather than the bar
//!   growing.
//!
//! The panel is *selectable*: `Ctrl+b m` moves the focus into it and the
//! keys it names then arrive here as `Event::Key`, which is why they need no
//! prefix. `Ctrl+b` chords keep working while it is focused — zellij
//! resolves those as a mode switch before the focused pane sees the key.
//!
//! Both instances receive every `sint-status`/`sint-ui` pipe (the sender
//! broadcasts rather than targeting a plugin url), so they share one
//! `State` transition table and stay in sync; each instance then answers
//! only for the pane it owns. What the *other* instance's pane is doing —
//! whether the panel exists at all, and who holds the focus — comes from
//! the pane manifest rather than bookkeeping, so a panel closed with
//! `Ctrl+b x` leaves the bar's idea of the world right.

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
use sint_zellij::state::{Effect, PanelKey, State, ThemeMode, PANEL_TITLE};

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct Plugin {
    st: State,
    /// True for the monitor-panel instance.
    is_panel: bool,
    /// This instance's own plugin pane id.
    own_id: u32,
    /// The shell pane the panel hands the focus back to.
    shell_pane: Option<u32>,
    /// The `sinteractive` binary, for the floating `top` view. It is
    /// usually not on the job's PATH, so the layout passes its full path.
    exe: String,
    /// The panel has asked for the focus but has not seen itself hold it —
    /// the layout it is created by focuses the shell, and which of the two
    /// lands last is not ours to decide, so the first manifest that says
    /// the shell won gets one retry.
    focus_pending: bool,
}

#[cfg(target_arch = "wasm32")]
register_plugin!(Plugin);

#[cfg(target_arch = "wasm32")]
const TICK_SECS: f64 = 0.5;

/// Context key marking the floating pane the panel opened for `t`, so its
/// exit can be told from any other command pane's.
#[cfg(target_arch = "wasm32")]
const TOP_CONTEXT: &str = "sint-top";

#[cfg(target_arch = "wasm32")]
impl ZellijPlugin for Plugin {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_panel = configuration.get("view").map(String::as_str) == Some("monitor");
        self.st.is_panel = self.is_panel;
        self.exe = configuration
            .get("exe")
            .cloned()
            .unwrap_or_else(|| "sinteractive".to_string());
        self.own_id = get_plugin_ids().plugin_id;
        // Subscribe before asking: with the permission pre-granted in
        // zellij's cache the answer arrives immediately, and an event sent
        // before the subscription is dropped.
        let mut events = vec![
            EventType::Timer,
            EventType::PermissionRequestResult,
            EventType::HostTerminalThemeChanged,
            EventType::PaneUpdate,
        ];
        if self.is_panel {
            events.push(EventType::Key);
            events.push(EventType::CommandPaneExited);
        }
        subscribe(&events);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            // `t` runs `sinteractive monitor` in a floating pane.
            PermissionType::RunCommands,
        ]);
        // Only the panel takes keys. It exists because someone asked for
        // it, so it takes the focus as it appears, and names itself so the
        // bar can find it in the manifest.
        set_selectable(self.is_panel);
        if self.is_panel {
            self.st.panel_open = true;
            self.focus_pending = true;
            rename_plugin_pane(self.own_id, PANEL_TITLE);
            focus_plugin_pane(self.own_id, false, false);
        }
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
                self.st.theme = match mode {
                    HostTerminalThemeMode::Light => ThemeMode::Light,
                    _ => ThemeMode::Dark,
                };
                true
            }
            Event::PaneUpdate(manifest) => self.on_panes(manifest),
            Event::Key(key) => {
                let Some(k) = key_name(&key).as_deref().and_then(PanelKey::from_name) else {
                    return false;
                };
                let effect = self.st.apply_key(k);
                self.run(effect);
                true
            }
            // Command panes are held open after the command exits; the
            // `top` view is a TUI the user has just quit, so take it away.
            Event::CommandPaneExited(pane_id, _exit, ctx) => {
                if ctx.contains_key(TOP_CONTEXT) {
                    close_terminal_pane(pane_id);
                }
                false
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
                    let effect = self.st.apply_action(a);
                    self.run(effect);
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
    /// Read this tab's panes: whether the panel is there, whether it holds
    /// the focus, and which terminal pane to give the focus back to.
    /// Returns true when the answer changed the picture we draw.
    fn on_panes(&mut self, manifest: PaneManifest) -> bool {
        let Some(panes) = manifest
            .panes
            .values()
            .find(|ps| ps.iter().any(|p| p.is_plugin && p.id == self.own_id))
        else {
            return false;
        };
        let (mut open, mut focused, mut shell) = (false, false, None);
        for p in panes {
            let is_panel_pane = p.is_plugin
                && if self.is_panel {
                    p.id == self.own_id
                } else {
                    // `contains`, not `==`: the title is what the UI would
                    // print, and zellij is free to dress it up.
                    p.id != self.own_id && p.title.contains(PANEL_TITLE)
                };
            if is_panel_pane {
                open = true;
                focused |= p.is_focused;
            } else if !p.is_plugin && !p.is_floating && !p.is_suppressed && shell.is_none() {
                shell = Some(p.id);
            }
        }
        let changed = self.st.panel_open != open || self.st.focused != focused;
        self.st.panel_open = open;
        self.st.focused = focused;
        self.shell_pane = shell;
        if self.focus_pending {
            self.focus_pending = false;
            if !focused {
                focus_plugin_pane(self.own_id, false, false);
            }
        }
        changed
    }

    /// Carry out what a keypress or a `sint-ui` action asked for.
    ///
    /// Opening re-applies the tab layout that includes the panel
    /// (`sint-panel`), retaining the terminal panes and the bar; zellij
    /// creates the panel instance in the layout's slot, between the shell
    /// and the bar, and that instance focuses itself as it loads. Closing
    /// is the panel closing itself, and its rows go back to the shell. (A
    /// layout swap on close would keep the retained panel instance alive
    /// and re-insert it beside the shell.)
    fn run(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::OpenPanel => {
                // Optimistic: the manifest confirms it a moment later, and
                // until then a second `Ctrl+b m` must not swap again.
                self.st.panel_open = true;
                override_layout(
                    LayoutInfo::File("sint-panel".to_string(), LayoutMetadata::default()),
                    true,
                    true,
                    true,
                    BTreeMap::new(),
                );
            }
            Effect::FocusPanel => focus_plugin_pane(self.own_id, false, false),
            Effect::FocusShell => match self.shell_pane {
                Some(id) => focus_terminal_pane(id, false, false),
                None => focus_next_pane(),
            },
            Effect::ClosePanel => close_self(),
            Effect::OpenTop => self.open_top(),
        }
    }

    /// `t`: the full `sinteractive monitor` TUI for the selected job, in a
    /// floating pane — the process table, sorted and scrollable, which is
    /// more than the panel's few rows can hold.
    fn open_top(&self) {
        let Some(job) = self.st.selected_job() else {
            return;
        };
        let mut ctx = BTreeMap::new();
        ctx.insert(TOP_CONTEXT.to_string(), job.to_string());
        open_command_pane_floating(
            CommandToRun {
                path: self.exe.clone().into(),
                args: vec!["monitor".to_string(), job.to_string()],
                cwd: None,
            },
            None,
            ctx,
        );
    }
}

/// Flatten a keypress into the names [`PanelKey`] knows. Anything with a
/// modifier is left alone: those belong to zellij's keybindings, and a
/// `Ctrl+b` chord never reaches a pane in the first place.
#[cfg(target_arch = "wasm32")]
fn key_name(k: &KeyWithModifier) -> Option<String> {
    if !k.key_modifiers.is_empty() {
        return None;
    }
    Some(match k.bare_key {
        BareKey::Left => "left".to_string(),
        BareKey::Right => "right".to_string(),
        BareKey::Enter => "enter".to_string(),
        BareKey::Esc => "esc".to_string(),
        BareKey::Char(c) => c.to_ascii_lowercase().to_string(),
        _ => return None,
    })
}
