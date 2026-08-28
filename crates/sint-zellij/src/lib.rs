//! `sint-zellij` — the one-line status bar (and the monitor panel beneath it)
//! that sinteractive shows at the bottom of every session.
//!
//! The plugin is deliberately dumb: everything it shows arrives as a
//! [`sint_proto::StatusMsg`] over the `sint-status` pipe from the native
//! sampler, and every keypress arrives as a [`sint_proto::UiAction`] over the
//! `sint-ui` pipe from the keybindings. Its own state is the bar mode, the
//! selected host, and animation frames. `render` is a pure function in
//! [`render`], unit-tested natively; only the thin `zellij_tile` glue in
//! `main.rs` is wasm-only.

pub mod render;
pub mod state;
