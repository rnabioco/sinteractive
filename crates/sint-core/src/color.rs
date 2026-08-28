//! ANSI palette, fd-aware.
//!
//! Mirrors `init_colors` in the 0.x script: narration goes to stderr, reports
//! to stdout, and each is decided separately so `sinteractive list | less`
//! carries no escapes while a plain `sinteractive list` is coloured. When
//! colour is off every code is the empty string, so there is exactly one set
//! of format strings. `SINTERACTIVE_COLOR=always` beats both `NO_COLOR` and
//! the tty test.

use crate::config::ColorMode;

/// The palette. Teal `#2DBFB8`-ish (`1;36`) for identifiers, echoing the
/// status bar.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub reset: &'static str,
    pub bold: &'static str,
    pub dim: &'static str,
    pub id: &'static str,
    pub hdr: &'static str,
    pub key: &'static str,
    pub ok: &'static str,
    pub warn: &'static str,
    pub err: &'static str,
}

impl Palette {
    /// Palette for the given fd (1 = stdout, 2 = stderr) under `mode`.
    pub fn for_fd(_mode: ColorMode, _fd: i32) -> Self {
        // TODO(phase-1/agent-B): tty test via libc isatty or std::io::IsTerminal.
        Palette::default()
    }

    pub fn none() -> Self {
        Palette::default()
    }
}
