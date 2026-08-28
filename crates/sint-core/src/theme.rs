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
//! The query runs at most once per process ([`Theme::detect`] is called once
//! per palette, and a command builds several), and it is skipped inside a
//! zellij pane: zellij forwards OSC 11 to the host terminal and waits a full
//! second for the answer, far longer than a CLI may stall, and the late reply
//! would then land in the shell as typed input.
//!
//! Colours are 24-bit; renderers that can only do 256 colours downsample with
//! [`Rgb::to_ansi256`].

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The six levels of the xterm 6x6x6 colour cube.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

impl Rgb {
    /// Nearest xterm-256 index (6x6x6 cube or grey ramp).
    ///
    /// Both candidates are scored by squared RGB distance and the closer wins;
    /// on a tie the cube is kept, so pure white stays 231 rather than a grey.
    pub fn to_ansi256(self) -> u8 {
        let Rgb(r, g, b) = self;
        let dist = |x: u8, y: u8| {
            let d = i32::from(x) - i32::from(y);
            d * d
        };
        let nearest_level = |v: u8| -> (usize, u8) {
            CUBE_LEVELS
                .iter()
                .enumerate()
                .map(|(i, &l)| (i, l))
                .min_by_key(|&(_, l)| dist(v, l))
                .unwrap_or((0, 0))
        };
        let (ri, rl) = nearest_level(r);
        let (gi, gl) = nearest_level(g);
        let (bi, bl) = nearest_level(b);
        let cube_index = 16 + 36 * ri + 6 * gi + bi;
        let cube_dist = dist(r, rl) + dist(g, gl) + dist(b, bl);

        // Grey ramp: 232..=255 are 8, 18, …, 238.
        let avg = (i32::from(r) + i32::from(g) + i32::from(b)) / 3;
        let step = (((avg - 8).max(0) + 5) / 10).clamp(0, 23);
        let grey_level = (8 + 10 * step) as u8;
        let grey_index = 232 + step as usize;
        let grey_dist = dist(r, grey_level) + dist(g, grey_level) + dist(b, grey_level);

        if grey_dist < cube_dist {
            grey_index as u8
        } else {
            cube_index as u8
        }
    }
    /// `\x1b[38;2;r;g;bm`
    pub fn fg_seq(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.0, self.1, self.2)
    }
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
    /// Perceived luminance in `0.0..=1.0` (Rec. 709 weights on the raw
    /// channels — enough to tell a dark background from a light one).
    pub fn luminance(self) -> f64 {
        let Rgb(r, g, b) = self;
        (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) / 255.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    /// `dark`/`light` (case-insensitive) → a mode; `auto` or anything else →
    /// `None`, meaning "detect".
    pub fn parse(value: &str) -> Option<Mode> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Mode::Dark),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }

    /// Mode for a terminal background colour: light when luminance > 0.5.
    pub fn from_background(bg: Rgb) -> Mode {
        if bg.luminance() > 0.5 {
            Mode::Light
        } else {
            Mode::Dark
        }
    }

    /// `COLORFGBG` (`fg;bg` or `fg;x;bg`): last field 0–6 or 8 → dark,
    /// 7 or 15 → light, anything else → `None`.
    pub fn from_colorfgbg(value: &str) -> Option<Mode> {
        let last = value.trim().rsplit(';').next()?;
        match last.trim().parse::<u8>().ok()? {
            0..=6 | 8 => Some(Mode::Dark),
            7 | 15 => Some(Mode::Light),
            _ => None,
        }
    }
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

/// How long the OSC 11 query may wait for the terminal's answer. It normally
/// returns as soon as the Device Attributes sentinel comes back — one round
/// trip — so this bounds only terminals that answer neither query, which is
/// rare enough (every terminal answers DA) to afford a margin wide enough for
/// a laggy ssh link: an answer that arrives after we stop reading is echoed
/// at the prompt as line noise.
pub const QUERY_TIMEOUT: Duration = Duration::from_millis(500);

impl Theme {
    /// Claude Code dark theme.
    pub const DARK: Theme = Theme {
        mode: Mode::Dark,
        accent: Rgb(0xD9, 0x77, 0x57),
        ok: Rgb(0x4E, 0xBA, 0x65),
        warn: Rgb(0xFF, 0xC1, 0x07),
        err: Rgb(0xFF, 0x6B, 0x80),
        dim: Rgb(0xCC, 0xCC, 0xCC),
        hint: Rgb(0xC8, 0xCE, 0xFF),
    };
    /// Claude Code light theme.
    pub const LIGHT: Theme = Theme {
        mode: Mode::Light,
        accent: Rgb(0xD9, 0x77, 0x57),
        ok: Rgb(0x2C, 0x7A, 0x39),
        warn: Rgb(0x96, 0x6C, 0x1E),
        err: Rgb(0xAB, 0x2B, 0x3F),
        dim: Rgb(0x55, 0x55, 0x55),
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
    ///
    /// Never hangs (the terminal query is bounded by [`QUERY_TIMEOUT`]) and
    /// never leaves the terminal in raw mode. Prints nothing except the query
    /// itself, which the terminal consumes.
    pub fn detect(fd: i32) -> Theme {
        Theme::for_mode(detect_mode(fd))
    }
}

fn detect_mode(fd: i32) -> Mode {
    if let Some(mode) = std::env::var("SINTERACTIVE_THEME")
        .ok()
        .and_then(|v| Mode::parse(&v))
    {
        return mode;
    }
    if is_tty(fd) {
        if let Some(bg) = background() {
            return Mode::from_background(bg);
        }
    }
    if let Some(mode) = std::env::var("COLORFGBG")
        .ok()
        .and_then(|v| Mode::from_colorfgbg(&v))
    {
        return mode;
    }
    Mode::Dark
}

/// The terminal's background colour, queried once per process.
///
/// Every command builds two or three palettes; one query each would stall
/// that many times over and, worse, leave that many windows in which a reply
/// arriving after the timeout is echoed at the prompt as line noise.
fn background() -> Option<Rgb> {
    static BACKGROUND: OnceLock<Option<Rgb>> = OnceLock::new();
    *BACKGROUND.get_or_init(|| query_background(QUERY_TIMEOUT))
}

/// `isatty(3)`; false for closed or invalid descriptors.
pub fn is_tty(fd: i32) -> bool {
    if fd < 0 {
        return false;
    }
    // SAFETY: isatty only inspects the descriptor; an invalid fd yields 0.
    unsafe { libc::isatty(fd) == 1 }
}

/// Restores the saved termios on drop, so every exit path from the query —
/// timeout, short read, parse failure, panic — puts the terminal back.
struct RawGuard {
    fd: i32,
    saved: libc::termios,
}

impl RawGuard {
    fn enable(fd: i32) -> Option<Self> {
        // SAFETY: termios is plain data; tcgetattr fills it or fails.
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return None;
        }
        let mut raw = saved;
        // No echo (the reply must not appear on screen), no line buffering
        // (the reply has no newline). Output processing is left alone.
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG | libc::IEXTEN);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return None;
        }
        Some(RawGuard { fd, saved })
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        // SAFETY: restoring the termios we read earlier on the same fd.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved);
        }
    }
}

/// Ask the controlling terminal for its background colour (OSC 11). `None`
/// when there is no controlling terminal, this process is not in the
/// foreground (reading would stop it with SIGTTIN), the terminal does not
/// answer within `timeout`, or the answer does not parse.
///
/// A Device Attributes query rides along behind the colour query. Terminals
/// answer queries in the order they arrive, so the DA reply is the sentinel
/// that says the colour answer either came already or is never coming: we
/// stop reading on it rather than on the clock, which both returns after one
/// round trip and leaves nothing behind to surface at the shell prompt as
/// line noise once the terminal is out of raw mode.
fn query_background(timeout: Duration) -> Option<Rgb> {
    if matches!(std::env::var("TERM").as_deref(), Ok("dumb") | Ok("")) {
        return None;
    }
    // Inside a zellij pane the query is not answered by the thing we are
    // talking to: zellij forwards it to the host terminal and gives that a
    // whole second, so the reply usually arrives long after any timeout a
    // CLI can afford — as bytes at the next prompt. The plugin gets the
    // mode from `HostTerminalThemeChanged` anyway; here `COLORFGBG` and the
    // env override are the whole story.
    if std::env::var_os("ZELLIJ").is_some() {
        return None;
    }
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();
    // SAFETY: plain queries on a descriptor we own.
    let foreground = unsafe { libc::tcgetpgrp(fd) == libc::getpgrp() };
    if !foreground {
        return None;
    }
    let _guard = RawGuard::enable(fd)?;
    (&tty).write_all(b"\x1b]11;?\x1b\\\x1b[c").ok()?;
    (&tty).flush().ok()?;

    let deadline = Instant::now() + timeout;
    let mut buf: Vec<u8> = Vec::with_capacity(64);
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let wait_ms = (deadline - now).as_millis().min(i32::MAX as u128) as i32;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd, count 1.
        let ready = unsafe { libc::poll(&mut pfd, 1, wait_ms) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if ready == 0 {
            break;
        }
        let mut chunk = [0u8; 64];
        // SAFETY: reading into a stack buffer of the stated length.
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
        // The sentinel has landed, so the colour reply is either in `buf` or
        // was never sent; either way there is nothing left to read.
        if has_da_reply(&buf) || buf.len() > 256 {
            break;
        }
    }
    // A terminal that answers the colour query but not the sentinel still
    // gets its answer used.
    match parse_osc11(&buf) {
        Osc11::Complete(rgb) => Some(rgb),
        _ => None,
    }
}

/// True once `buf` holds a Device Attributes reply — a CSI sequence whose
/// final byte is `c` (`ESC [ ? 6 c` and friends).
pub fn has_da_reply(buf: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] != 0x1b || buf[i + 1] != b'[' {
            i += 1;
            continue;
        }
        // Parameter and intermediate bytes (0x20..=0x3F) up to the final
        // byte, which is the first in 0x40..=0x7E.
        let mut j = i + 2;
        while j < buf.len() && (0x20..=0x3f).contains(&buf[j]) {
            j += 1;
        }
        if j >= buf.len() {
            return false;
        }
        if buf[j] == b'c' {
            return true;
        }
        i = j + 1;
    }
    false
}

/// Outcome of parsing a partial OSC 11 reply.
#[derive(Debug, PartialEq, Eq)]
pub enum Osc11 {
    /// A full reply was present and parsed.
    Complete(Rgb),
    /// No terminator yet — keep reading.
    Incomplete,
    /// A terminated reply that did not parse.
    Invalid,
}

/// Parse `\x1b]11;rgb:RRRR/GGGG/BBBB` terminated by `\x1b\\` (ST) or `\x07`
/// (BEL). Components may be 1–4 hex digits and are scaled to 8 bits;
/// `rgba:` replies contribute their first three components.
pub fn parse_osc11(buf: &[u8]) -> Osc11 {
    // Terminated?
    let st = buf.windows(2).position(|w| w == b"\x1b\\");
    let bel = buf.iter().position(|&b| b == 0x07);
    let end = match (st, bel) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return Osc11::Incomplete,
    };
    let text = String::from_utf8_lossy(&buf[..end]);
    let Some(pos) = text.find("rgb") else {
        return Osc11::Invalid;
    };
    let rest = &text[pos..];
    let Some((_, spec)) = rest.split_once(':') else {
        return Osc11::Invalid;
    };
    let parts: Vec<&str> = spec.split('/').collect();
    if parts.len() < 3 {
        return Osc11::Invalid;
    }
    let scale = |s: &str| -> Option<u8> {
        let s = s.trim();
        if s.is_empty() || s.len() > 4 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        let max = (1u32 << (4 * s.len() as u32)) - 1;
        Some(((v * 255 + max / 2) / max) as u8)
    };
    match (scale(parts[0]), scale(parts[1]), scale(parts[2])) {
        (Some(r), Some(g), Some(b)) => Osc11::Complete(Rgb(r, g, b)),
        _ => Osc11::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_env::{lock, EnvRestore};

    #[test]
    fn ansi256_primaries_and_greys() {
        assert_eq!(Rgb(255, 0, 0).to_ansi256(), 196, "pure red");
        assert_eq!(Rgb(0, 255, 0).to_ansi256(), 46, "pure green");
        assert_eq!(Rgb(0, 0, 255).to_ansi256(), 21, "pure blue");
        assert_eq!(
            Rgb(255, 255, 255).to_ansi256(),
            231,
            "white stays in the cube"
        );
        assert_eq!(Rgb(0, 0, 0).to_ansi256(), 16, "black");
        assert_eq!(Rgb(128, 128, 128).to_ansi256(), 244, "mid grey on the ramp");
        assert_eq!(Rgb(8, 8, 8).to_ansi256(), 232, "darkest ramp grey");
        assert_eq!(Rgb(238, 238, 238).to_ansi256(), 255, "lightest ramp grey");
        assert_eq!(Rgb(0x99, 0x99, 0x99).to_ansi256(), 247);
    }

    #[test]
    fn luminance_splits_dark_from_light() {
        assert_eq!(Mode::from_background(Rgb(0, 0, 0)), Mode::Dark);
        assert_eq!(Mode::from_background(Rgb(255, 255, 255)), Mode::Light);
        assert_eq!(Mode::from_background(Rgb(0x1E, 0x1E, 0x1E)), Mode::Dark);
        assert_eq!(Mode::from_background(Rgb(0xFD, 0xF6, 0xE3)), Mode::Light);
    }

    #[test]
    fn colorfgbg_parsing() {
        assert_eq!(Mode::from_colorfgbg("15;0"), Some(Mode::Dark));
        assert_eq!(Mode::from_colorfgbg("0;15"), Some(Mode::Light));
        assert_eq!(Mode::from_colorfgbg("0;default;7"), Some(Mode::Light));
        assert_eq!(Mode::from_colorfgbg("7;8"), Some(Mode::Dark));
        assert_eq!(Mode::from_colorfgbg("7;9"), None);
        assert_eq!(Mode::from_colorfgbg(""), None);
        assert_eq!(Mode::from_colorfgbg("default"), None);
    }

    #[test]
    fn device_attributes_sentinel() {
        assert!(has_da_reply(b"\x1b[?6c"));
        assert!(has_da_reply(b"\x1b[?62;1;4c"));
        assert!(has_da_reply(b"\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\\x1b[?6c"));
        assert!(has_da_reply(b"\x1b[0m\x1b[?1;2c"), "after another CSI");
        assert!(!has_da_reply(b"\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\"));
        assert!(!has_da_reply(b"\x1b[?6"), "still arriving");
        assert!(!has_da_reply(b"\x1b[0m"), "some other CSI");
        assert!(!has_da_reply(b""));
    }

    #[test]
    fn osc11_replies() {
        assert_eq!(
            parse_osc11(b"\x1b]11;rgb:0000/0000/0000\x1b\\"),
            Osc11::Complete(Rgb(0, 0, 0))
        );
        assert_eq!(
            parse_osc11(b"\x1b]11;rgb:ffff/ffff/ffff\x07"),
            Osc11::Complete(Rgb(255, 255, 255))
        );
        assert_eq!(
            parse_osc11(b"\x1b]11;rgb:1e/1e/2e\x1b\\"),
            Osc11::Complete(Rgb(0x1e, 0x1e, 0x2e))
        );
        assert_eq!(
            parse_osc11(b"\x1b]11;rgba:ffff/0000/8080/ffff\x1b\\"),
            Osc11::Complete(Rgb(255, 0, 0x80))
        );
        assert_eq!(parse_osc11(b"\x1b]11;rgb:ffff/ff"), Osc11::Incomplete);
        assert_eq!(parse_osc11(b""), Osc11::Incomplete);
        assert_eq!(parse_osc11(b"\x1b]11;?\x1b\\"), Osc11::Invalid);
        assert_eq!(parse_osc11(b"\x1b]11;rgb:zz/zz/zz\x07"), Osc11::Invalid);
    }

    #[test]
    fn detect_honours_env_override_on_a_non_tty() {
        let _g = lock();
        let _r = EnvRestore::clean();
        let file = tempfile::tempfile().expect("tempfile");
        let fd = file.as_raw_fd();
        assert!(!is_tty(fd));

        std::env::set_var("SINTERACTIVE_THEME", "light");
        assert_eq!(Theme::detect(fd), Theme::LIGHT);
        std::env::set_var("SINTERACTIVE_THEME", "DARK");
        assert_eq!(Theme::detect(fd), Theme::DARK);

        // auto → no tty → COLORFGBG → dark.
        std::env::set_var("SINTERACTIVE_THEME", "auto");
        std::env::set_var("COLORFGBG", "0;15");
        assert_eq!(Theme::detect(fd).mode, Mode::Light);
        std::env::set_var("COLORFGBG", "15;0");
        assert_eq!(Theme::detect(fd).mode, Mode::Dark);
        std::env::remove_var("COLORFGBG");
        assert_eq!(Theme::detect(fd), Theme::DARK);
        assert_eq!(Theme::detect(-1), Theme::DARK);
    }

    #[test]
    fn theme_lookup() {
        assert_eq!(Theme::for_mode(Mode::Light).mode, Mode::Light);
        assert_eq!(Theme::for_mode(Mode::Dark), Theme::DARK);
        assert_eq!(Rgb(0xD9, 0x77, 0x57).hex(), "#D97757");
        assert_eq!(Rgb(1, 2, 3).fg_seq(), "\x1b[38;2;1;2;3m");
    }
}
