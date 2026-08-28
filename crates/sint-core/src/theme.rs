//! Colour theme — Claude Code's palette, dark/light aware.
//!
//! One source of truth for every renderer: the CLI palette ([`crate::color`]),
//! the ratatui views, the zellij status plugin, and the Claude statusline.
//! Values follow Claude Code's own dark and light themes so the tool reads as
//! part of the same family; the accent is Claude's orange in both modes.
//!
//! Mode detection:
//! - `SINTERACTIVE_THEME=dark|light|auto` (default `auto`) always wins
//! - inside zellij the plugin receives `HostTerminalThemeChanged` and uses that
//! - the CLI queries the terminal background (OSC 11 with a short timeout),
//!   then falls back to `COLORFGBG`, then to dark
//!
//! Colours are 24-bit; renderers that can only do 256 colours downsample with
//! [`Rgb::to_ansi256`].

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Nearest xterm-256 index (6x6x6 cube or grey ramp).
    pub fn to_ansi256(self) -> u8 {
        // TODO(phase-1/agent-B)
        unimplemented!()
    }
    /// `\x1b[38;2;r;g;bm`
    pub fn fg_seq(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.0, self.1, self.2)
    }
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

/// Semantic colours. Text colours only — backgrounds stay the terminal's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub mode: Mode,
    /// Claude orange — session glyph, identifiers, headings.
    pub accent: Rgb,
    /// Success / healthy / RUNNING.
    pub ok: Rgb,
    /// Warning / yellow phase / PENDING.
    pub warn: Rgb,
    /// Error / red phase / quota.
    pub err: Rgb,
    /// Secondary text, separators, labels.
    pub dim: Rgb,
    /// Keys and hints (Claude's "permission"/suggestion tint).
    pub hint: Rgb,
}

impl Theme {
    /// Claude Code dark theme.
    pub const DARK: Theme = Theme {
        mode: Mode::Dark,
        accent: Rgb(0xD9, 0x77, 0x57),
        ok: Rgb(0x4E, 0xBA, 0x65),
        warn: Rgb(0xFF, 0xC1, 0x07),
        err: Rgb(0xFF, 0x6B, 0x80),
        dim: Rgb(0x99, 0x99, 0x99),
        hint: Rgb(0xB1, 0xB9, 0xF9),
    };
    /// Claude Code light theme.
    pub const LIGHT: Theme = Theme {
        mode: Mode::Light,
        accent: Rgb(0xD9, 0x77, 0x57),
        ok: Rgb(0x2C, 0x7A, 0x39),
        warn: Rgb(0x96, 0x6C, 0x1E),
        err: Rgb(0xAB, 0x2B, 0x3F),
        dim: Rgb(0x66, 0x66, 0x66),
        hint: Rgb(0x57, 0x69, 0xF7),
    };

    pub fn for_mode(mode: Mode) -> Theme {
        match mode {
            Mode::Dark => Theme::DARK,
            Mode::Light => Theme::LIGHT,
        }
    }

    /// Resolve the mode for a CLI process: env override, then terminal
    /// query on `fd` if it is a tty, then `COLORFGBG`, then dark.
    pub fn detect(_fd: i32) -> Theme {
        // TODO(phase-1/agent-B): implement per module docs; keep the OSC 11
        // query under ~100 ms and never hang on a non-responding terminal.
        Theme::DARK
    }
}
