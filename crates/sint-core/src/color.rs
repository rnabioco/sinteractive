//! ANSI palette, fd-aware.
//!
//! Mirrors `init_colors` in the 0.x script: narration goes to stderr, reports
//! to stdout, and each is decided separately so `sinteractive list | less`
//! carries no escapes while a plain `sinteractive list` is coloured. When
//! colour is off every code is the empty string, so there is exactly one set
//! of format strings. `SINTERACTIVE_COLOR=always` beats both `NO_COLOR` and
//! the tty test.
//!
//! Colours come from [`crate::theme`] as 24-bit sequences, chosen for the
//! detected dark or light background; the 0.x fixed `1;36` teal is gone.

use crate::config::ColorMode;
use crate::theme::{self, Theme};

/// The palette. `id`/`hdr` carry the accent, `key` the hint tint, the rest
/// their semantic colour; every field is empty when colour is off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    pub reset: String,
    pub bold: String,
    pub dim: String,
    pub id: String,
    pub hdr: String,
    pub key: String,
    pub ok: String,
    pub warn: String,
    pub err: String,
}

impl Palette {
    /// Palette for the given fd (1 = stdout, 2 = stderr) under `mode`.
    pub fn for_fd(mode: ColorMode, fd: i32) -> Self {
        if !colour_wanted(mode, fd) {
            return Palette::none();
        }
        Palette::from_theme(&Theme::detect(fd))
    }

    /// Every code from `theme`, regardless of the fd.
    pub fn from_theme(theme: &Theme) -> Self {
        Palette {
            reset: "\x1b[0m".to_string(),
            bold: "\x1b[1m".to_string(),
            dim: theme.dim.fg_seq(),
            id: theme.accent.fg_seq(),
            hdr: theme.accent.fg_seq(),
            key: theme.hint.fg_seq(),
            ok: theme.ok.fg_seq(),
            warn: theme.warn.fg_seq(),
            err: theme.err.fg_seq(),
        }
    }

    pub fn none() -> Self {
        Palette::default()
    }

    /// True when any code is set.
    pub fn is_enabled(&self) -> bool {
        !self.reset.is_empty()
    }
}

/// The `init_colors` decision: `always` wins outright, `never` loses
/// outright, and `auto` needs a tty, no `NO_COLOR` (any value), and a `TERM`
/// that is set and not `dumb`.
fn colour_wanted(mode: ColorMode, fd: i32) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            if std::env::var_os("NO_COLOR").is_some() {
                return false;
            }
            if !theme::is_tty(fd) {
                return false;
            }
            !matches!(
                std::env::var("TERM").as_deref(),
                Ok("dumb") | Ok("") | Err(_)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_env::{lock, EnvRestore};
    use std::os::unix::io::AsRawFd;

    #[test]
    fn never_is_empty() {
        let _g = lock();
        let _r = EnvRestore::clean();
        let p = Palette::for_fd(ColorMode::Never, 1);
        assert_eq!(p, Palette::none());
        assert!(!p.is_enabled());
    }

    #[test]
    fn auto_on_a_non_tty_is_empty() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("TERM", "xterm-256color");
        let file = tempfile::tempfile().expect("tempfile");
        assert_eq!(
            Palette::for_fd(ColorMode::Auto, file.as_raw_fd()),
            Palette::none()
        );
    }

    #[test]
    fn always_uses_the_env_theme_even_without_a_tty() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("NO_COLOR", "");
        std::env::set_var("SINTERACTIVE_THEME", "light");
        let file = tempfile::tempfile().expect("tempfile");
        let p = Palette::for_fd(ColorMode::Always, file.as_raw_fd());
        assert!(p.is_enabled());
        assert_eq!(p.id, Theme::LIGHT.accent.fg_seq());
        assert_eq!(p.hdr, Theme::LIGHT.accent.fg_seq());
        assert_eq!(p.key, Theme::LIGHT.hint.fg_seq());
        assert_eq!(p.ok, Theme::LIGHT.ok.fg_seq());
        assert_eq!(p.warn, Theme::LIGHT.warn.fg_seq());
        assert_eq!(p.err, Theme::LIGHT.err.fg_seq());
        assert_eq!(p.dim, Theme::LIGHT.dim.fg_seq());
        assert_eq!(p.reset, "\x1b[0m");
        assert_eq!(p.bold, "\x1b[1m");

        std::env::set_var("SINTERACTIVE_THEME", "dark");
        let p = Palette::for_fd(ColorMode::Always, file.as_raw_fd());
        assert_eq!(p.ok, Theme::DARK.ok.fg_seq());
    }

    #[test]
    fn no_color_and_dumb_term_disable_auto() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("TERM", "xterm");
        std::env::set_var("NO_COLOR", "1");
        assert!(!colour_wanted(ColorMode::Auto, 1));
        std::env::remove_var("NO_COLOR");
        std::env::set_var("TERM", "dumb");
        assert!(!colour_wanted(ColorMode::Auto, 1));
        assert!(colour_wanted(ColorMode::Always, -1));
        assert!(!colour_wanted(ColorMode::Never, 1));
    }
}
