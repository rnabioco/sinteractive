//! Pure rendering of the bar and the monitor panel to ANSI strings.
//!
//! Colours follow Claude Code's palette (see `sint-core::theme`; duplicated
//! here as constants because this crate must stay wasm-clean and tiny).
//! Only foreground colours are set; the background is the terminal's.

use unicode_width::UnicodeWidthStr;

use crate::state::{BarMode, State, ThemeMode};
use sint_proto::Severity;

#[derive(Clone, Copy)]
pub struct Colors {
    pub accent: (u8, u8, u8),
    pub ok: (u8, u8, u8),
    pub warn: (u8, u8, u8),
    pub err: (u8, u8, u8),
    /// Values. Set on every one of them, because a cell this plugin leaves
    /// unstyled takes zellij's theme `fg`, a mid grey dimmer than the labels.
    pub text: (u8, u8, u8),
    pub dim: (u8, u8, u8),
    /// The unfilled half of a gauge — below `dim`, so a trough never
    /// outshines the words beside it.
    pub track: (u8, u8, u8),
    pub hint: (u8, u8, u8),
}

pub const DARK: Colors = Colors {
    accent: (0xD9, 0x77, 0x57),
    ok: (0x4E, 0xBA, 0x65),
    warn: (0xFF, 0xC1, 0x07),
    err: (0xFF, 0x6B, 0x80),
    text: (0xFF, 0xFF, 0xFF),
    dim: (0xD9, 0xD9, 0xD9),
    track: (0x59, 0x59, 0x59),
    hint: (0xD7, 0xE0, 0xFF),
};

pub const LIGHT: Colors = Colors {
    accent: (0xD9, 0x77, 0x57),
    ok: (0x2C, 0x7A, 0x39),
    warn: (0x96, 0x6C, 0x1E),
    err: (0xAB, 0x2B, 0x3F),
    text: (0x1A, 0x1A, 0x1A),
    dim: (0x3F, 0x3F, 0x3F),
    track: (0xA6, 0xA6, 0xA6),
    hint: (0x57, 0x69, 0xF7),
};

pub fn colors(theme: ThemeMode) -> Colors {
    match theme {
        ThemeMode::Dark => DARK,
        ThemeMode::Light => LIGHT,
    }
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
/// Back to normal intensity without dropping the colour (`RESET` would).
const NOBOLD: &str = "\x1b[22m";

fn fg(c: (u8, u8, u8)) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}

/// The heavy accent rule that fences the sint panes off from the shell.
///
/// 0.x drew this with tmux's bottom pane border (`pane-border-lines heavy`,
/// `fg=yellow`); zellij's panes are borderless, so each sint pane draws it as
/// its own first row — the bar always, the panel too when it is open, which
/// leaves the open panel framed between two rules.
pub fn rule(cols: usize, c: &Colors) -> String {
    format!("{}{}{RESET}", fg(c.accent), "\u{2501}".repeat(cols))
}

/// Braille spinner frames used in the red phase (single-width, no jitter).
pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Strip ANSI escapes.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // CSI: ESC [ … final byte in 0x40..=0x7E
            if chars.next() == Some('[') {
                for d in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&d) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Printed width of `s` once escapes are removed.
pub fn visible_width(s: &str) -> usize {
    UnicodeWidthStr::width(strip_ansi(s).as_str())
}

/// The session glyph for this frame.
pub fn glyph(sev: Severity, frame: u64, c: &Colors) -> String {
    match sev {
        Severity::Ok => format!("{}{BOLD}●{RESET}", fg(c.accent)),
        Severity::Yellow => {
            if frame % 2 == 0 {
                format!("{}{BOLD}●{RESET}", fg(c.warn))
            } else {
                format!("{}●{RESET}", fg(c.dim))
            }
        }
        Severity::Red => {
            let f = SPINNER[(frame as usize) % SPINNER.len()];
            let col = if frame % 4 < 2 { c.err } else { c.warn };
            format!("{}{BOLD}{f}{RESET}", fg(col))
        }
        Severity::Ending => format!("{}{BOLD}■{RESET}", fg(c.err)),
    }
}

/// One segment of the status line: text plus the priority at which it is
/// dropped when the line is too wide (higher = dropped first).
struct Seg {
    text: String,
    drop_prio: u8,
}

/// Build the status-mode line for `cols` columns.
pub fn status_line(st: &State, cols: usize) -> String {
    let c = colors(st.theme);
    let m = &st.msg;
    let sep = format!(" {}·{RESET} ", fg(c.dim));
    let mut segs: Vec<Seg> = Vec::new();
    let id = format!(
        "{}sint{RESET} {}{BOLD}{}{RESET}",
        fg(c.dim),
        fg(c.accent),
        m.job_id
    );
    segs.push(Seg {
        text: format!("{} {id}", glyph(m.severity, st.frame, &c)),
        drop_prio: 0,
    });
    if let Some(n) = &m.name {
        segs.push(Seg {
            text: format!("{}{n}{RESET}", fg(c.accent)),
            drop_prio: 5,
        });
    }
    if !m.host.is_empty() {
        segs.push(Seg {
            text: format!("{}{}{RESET}", fg(c.text), m.host),
            drop_prio: 4,
        });
    }
    if !m.remaining.is_empty() {
        let value = match m.severity {
            Severity::Ok => format!("{}{}{RESET}", fg(c.text), m.remaining),
            Severity::Yellow => format!("{}{}{RESET}", fg(c.warn), m.remaining),
            Severity::Red | Severity::Ending => {
                format!("{}{BOLD}{}{RESET}", fg(c.err), m.remaining)
            }
        };
        segs.push(Seg {
            text: format!("{value} {}left{RESET}", fg(c.dim)),
            drop_prio: 1,
        });
    }
    if !m.load.is_empty() {
        segs.push(Seg {
            text: format!("{}{}{RESET}", fg(c.text), m.load),
            drop_prio: 3,
        });
    }
    if !m.gpu.is_empty() {
        segs.push(Seg {
            text: format!("{}{}{RESET}", fg(c.text), m.gpu),
            drop_prio: 3,
        });
    }
    if !m.jobs.is_empty() {
        segs.push(Seg {
            text: format!("{}jobs{RESET} {}{}{RESET}", fg(c.dim), fg(c.text), m.jobs),
            drop_prio: 4,
        });
    }
    if !m.hosts.is_empty() && !st.panel_open {
        let n = m.hosts.len();
        segs.push(Seg {
            text: format!(
                "{}{BOLD}▣ {n} job{} monitorable{RESET} {}^b m{RESET}",
                fg(c.hint),
                if n == 1 { "" } else { "s" },
                fg(c.dim)
            ),
            drop_prio: 6,
        });
    }
    if !m.notices.is_empty() {
        let n = m.notices.len();
        let severe = m.notices.iter().any(|x| x.kind == "quota");
        let col = if severe { c.err } else { c.warn };
        let label = format!("{n} notice{}", if n == 1 { "" } else { "s" });
        let label = if severe {
            shimmer(&label, st.frame, col, [c.accent, c.warn, c.accent])
        } else {
            format!("{}{label}{RESET}", fg(col))
        };
        segs.push(Seg {
            text: format!("{}⚠ {RESET}{label} {}^b n{RESET}", fg(col), fg(c.dim)),
            drop_prio: 2,
        });
    }
    segs.push(Seg {
        text: format!("{}^b h help{RESET}", fg(c.dim)),
        drop_prio: 7,
    });

    // Drop segments from the highest priority down until it fits.
    let mut keep: Vec<bool> = vec![true; segs.len()];
    loop {
        let line: Vec<&str> = segs
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(s, _)| s.text.as_str())
            .collect();
        let joined = line.join(&sep);
        if visible_width(&joined) <= cols || line.len() <= 1 {
            return joined;
        }
        let (i, _) = segs
            .iter()
            .enumerate()
            .filter(|(i, _)| keep[*i])
            .max_by_key(|(_, s)| s.drop_prio)
            .unwrap();
        keep[i] = false;
    }
}

/// A band of `band.len()` characters sweeping across `text`, one column per
/// frame, sliding in from the left.
///
/// The band is bold and carries its own colours cell by cell, so an ember
/// runs along the word — red underneath, orange into yellow and back at the
/// crest — rather than the two-tone blink a single band colour gives, which
/// on a status line a metre away is easy to miss.
pub fn shimmer(text: &str, frame: u64, base: (u8, u8, u8), band: [(u8, u8, u8); 3]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as i64;
    let band_w = band.len() as i64;
    let pos = (frame as i64 % (len + band_w)) - band_w;
    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        let i = i as i64;
        let lit = i >= pos && i < pos + band_w;
        out.push_str(if lit { BOLD } else { NOBOLD });
        out.push_str(&fg(if lit { band[(i - pos) as usize] } else { base }));
        out.push(*ch);
    }
    out.push_str(RESET);
    out
}

pub fn notices_line(st: &State, idx: usize, cols: usize) -> String {
    let c = colors(st.theme);
    let n = st.msg.notices.len();
    let Some(notice) = st.msg.notices.get(idx) else {
        return status_line(st, cols);
    };
    let col = if notice.kind == "quota" {
        c.err
    } else {
        c.warn
    };
    let hint = format!("{}^b n next · ^b esc back{RESET}", fg(c.dim));
    let head = format!("{}⚠ {}/{n}{RESET}  ", fg(col), idx + 1);
    // The hint goes first on a narrow bar: the notice itself is the point,
    // and a truncated one-word notice is worse than no key legend.
    let head_w = visible_width(&head);
    let with_hint = head_w + 3 + visible_width(&hint);
    let (tail, fixed) = if with_hint + 8 <= cols {
        (format!("   {hint}"), with_hint)
    } else {
        (String::new(), head_w)
    };
    let room = cols.saturating_sub(fixed);
    let mut text = notice.text.clone();
    if visible_width(&text) > room {
        text = notice.text.chars().take(room.saturating_sub(1)).collect();
        text.push('…');
    }
    format!("{head}{}{text}{RESET}{tail}", fg(col))
}

pub fn help_line(st: &State, page: usize, cols: usize) -> String {
    let c = colors(st.theme);
    let pages = st.msg.help.len().max(1);
    let Some(entries) = st.msg.help.get(page) else {
        return status_line(st, cols);
    };
    let mut parts = vec![format!("{}{BOLD}^b{RESET}", fg(c.accent))];
    for (k, d) in entries {
        parts.push(format!("{}{k}{RESET} {}{d}{RESET}", fg(c.hint), fg(c.dim)));
    }
    let mut line = parts.join(&format!(" {}·{RESET} ", fg(c.dim)));
    let tail = format!("   {}({}/{pages}){RESET}", fg(c.dim), page + 1);
    if visible_width(&line) + visible_width(&tail) > cols {
        // Trim entries from the end until it fits.
        while parts.len() > 2 && visible_width(&parts.join(" · ")) + visible_width(&tail) > cols {
            parts.pop();
        }
        line = parts.join(&format!(" {}·{RESET} ", fg(c.dim)));
    }
    format!("{line}{tail}")
}

/// A horizontal bar of `width` cells for `pct` (0–100).
pub fn bar(pct: u8, width: usize, col: (u8, u8, u8), track: (u8, u8, u8)) -> String {
    let filled = (width * pct.min(100) as usize).div_ceil(100);
    let mut s = fg(col);
    s.push_str(&"█".repeat(filled));
    s.push_str(&fg(track));
    s.push_str(&"░".repeat(width.saturating_sub(filled)));
    s.push_str(RESET);
    s
}

/// A sparkline from 0–100 samples.
pub fn sparkline(samples: &[u8], width: usize) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let start = samples.len().saturating_sub(width);
    samples[start..]
        .iter()
        .map(|v| BLOCKS[((*v as usize).min(100) * 7) / 100])
        .collect()
}

/// `used` as a percentage of `total`, clamped to 100; 0 when `total` is 0.
fn pct_of(used: u64, total: u64) -> u8 {
    (used * 100).checked_div(total).unwrap_or(0).min(100) as u8
}

fn mb_to_g(mb: u64) -> String {
    let g = mb as f64 / 1024.0;
    if g >= 10.0 {
        format!("{g:.0}G")
    } else {
        format!("{g:.1}G")
    }
}

/// The monitor panel rows (excluding the status line), `rows` high.
pub fn panel_lines(st: &State, rows: usize, cols: usize) -> Vec<String> {
    let c = colors(st.theme);
    let mut out = Vec::new();
    if rows == 0 {
        return out;
    }
    let n = st.msg.hosts.len();
    let Some(h) = st.msg.hosts.get(st.host_idx) else {
        out.push(format!(
            "{}no monitorable jobs — start one and it appears here{RESET}",
            fg(c.dim)
        ));
        return out;
    };
    // Selector row.
    let name = h.job_name.as_deref().unwrap_or("");
    let stale = if h.age_secs > 30 {
        format!(" {}({}s old){RESET}", fg(c.warn), h.age_secs)
    } else {
        String::new()
    };
    out.push(format!(
        "{}◀{RESET} {}{BOLD}{}{RESET} {}·{RESET} {}{} {}{stale} {}{}/{n}{RESET} {}▶{RESET}   {}^b ,/. host · ^b m close{RESET}",
        fg(c.hint),
        fg(c.accent),
        h.host,
        fg(c.dim),
        fg(c.text),
        h.job_id,
        name,
        fg(c.dim),
        st.host_idx + 1,
        fg(c.hint),
        fg(c.dim)
    ));
    // Resource rows.
    let bw = 20usize.min(cols.saturating_sub(30).max(5));
    out.push(format!(
        "{}cpu{RESET} {} {}{:>3}%{RESET} {}of{RESET} {}{} {}·{RESET} {}load{RESET} {}{:.1}{RESET}  {}{}{RESET}",
        fg(c.dim),
        bar(h.cpu_pct, bw, c.ok, c.track),
        fg(c.text),
        h.cpu_pct,
        fg(c.dim),
        fg(c.text),
        h.cpu_alloc,
        fg(c.dim),
        fg(c.dim),
        fg(c.text),
        h.load1,
        fg(c.dim),
        sparkline(&h.cpu_history, cols.saturating_sub(bw + 40).min(30))
    ));
    let mem_pct = pct_of(h.mem_used_mb, h.mem_alloc_mb);
    out.push(format!(
        "{}mem{RESET} {} {}{:>3}% {}{RESET} {}/{RESET} {}{}{RESET}",
        fg(c.dim),
        bar(mem_pct, bw, c.accent, c.track),
        fg(c.text),
        mem_pct,
        mb_to_g(h.mem_used_mb),
        fg(c.dim),
        fg(c.text),
        mb_to_g(h.mem_alloc_mb)
    ));
    for g in &h.gpus {
        let mem_pct = pct_of(g.mem_used_mb, g.mem_total_mb);
        let extra = [
            g.temp_c.map(|t| format!("{t}°C")),
            g.power_w.map(|p| format!("{p}W")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        out.push(format!(
            "{}gpu{}{RESET} {} {}{:>3}%{RESET} {}{}/{}{RESET} {}{}{RESET} {}",
            fg(c.dim),
            g.index,
            bar(g.util_pct, bw, c.warn, c.track),
            fg(c.text),
            g.util_pct,
            fg(c.dim),
            mb_to_g(g.mem_used_mb),
            mb_to_g(g.mem_total_mb),
            fg(c.text),
            extra,
            bar(mem_pct, 8, c.accent, c.track)
        ));
    }
    // Process table in the remaining rows.
    let remaining = rows.saturating_sub(out.len());
    if remaining >= 2 && !h.procs.is_empty() {
        out.push(format!(
            "{}{:>7} {:>5} {:>7} {:>6}  {}{RESET}",
            fg(c.dim),
            "PID",
            "CPU%",
            "RSS",
            "GPU",
            "COMMAND"
        ));
        for p in h.procs.iter().take(remaining - 1) {
            let gpu = p.gpu_mem_mb.map(mb_to_g).unwrap_or_else(|| "-".into());
            let cmd_room = cols.saturating_sub(31);
            let cmd: String = p.command.chars().take(cmd_room).collect();
            out.push(format!(
                "{}{:>7} {:>5.1} {:>7} {:>6}  {cmd}{RESET}",
                fg(c.text),
                p.pid,
                p.cpu_pct,
                mb_to_g(p.rss_mb),
                gpu
            ));
        }
    }
    out.truncate(rows);
    out
}

/// Render the monitor-panel pane (the `view=monitor` instance): the rule and
/// the panel rows, no status line.
pub fn render_panel(st: &State, rows: usize, cols: usize) -> String {
    let mut out = vec![rule(cols, &colors(st.theme))];
    out.extend(panel_lines(st, rows.saturating_sub(1), cols));
    out.join("\n")
}

/// Render the bar pane: the bar line, then the panel when open and there is
/// room (single-instance fallback).
pub fn render(st: &State, rows: usize, cols: usize) -> String {
    let line = match st.mode {
        BarMode::Status => status_line(st, cols),
        BarMode::Notices { idx } => notices_line(st, idx, cols),
        BarMode::Help { page } => help_line(st, page, cols),
    };
    // Row 0 is the rule whenever the bar has room for it. The bar is two rows
    // in both layouts, so with the panel open the region reads as a framed
    // block: rule, panel, rule, status line. (In the single-pane fallback the
    // bar draws the panel itself, below the line.)
    let mut out: Vec<String> = Vec::new();
    if rows > 1 {
        out.push(rule(cols, &colors(st.theme)));
    }
    out.push(line);
    if st.panel_open && rows > out.len() {
        out.extend(panel_lines(st, rows - out.len(), cols));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sint_proto::{GpuLine, HostPanel, Notice, ProcLine, StatusMsg};

    fn state() -> State {
        let mut st = State::default();
        st.apply_msg(StatusMsg {
            job_id: 147845,
            name: Some("mywork".into()),
            host: "c3gpu-a5-u1".into(),
            severity: Severity::Ok,
            remaining: "2h 41m".into(),
            load: "cpu 34% 12/32G".into(),
            gpu: "gpu0 87% 31/40G".into(),
            jobs: "2R 1PD".into(),
            notices: vec![Notice {
                kind: "quota".into(),
                text: "QUOTA over by 12G (500G limit)".into(),
            }],
            help: vec![vec![
                ("n".into(), "notices".into()),
                ("h".into(), "help".into()),
            ]],
            hosts: vec![HostPanel {
                host: "c3gpu-a5-u1".into(),
                job_id: 147845,
                job_name: Some("mywork".into()),
                age_secs: 3,
                cpu_pct: 34,
                cpu_alloc: 8,
                mem_used_mb: 12288,
                mem_alloc_mb: 32768,
                load1: 3.2,
                gpus: vec![GpuLine {
                    index: 0,
                    name: "A100".into(),
                    util_pct: 87,
                    mem_used_mb: 31744,
                    mem_total_mb: 40960,
                    temp_c: Some(61),
                    power_w: Some(240),
                }],
                procs: vec![ProcLine {
                    pid: 4242,
                    user: "jay".into(),
                    cpu_pct: 312.5,
                    rss_mb: 8192,
                    gpu_mem_mb: Some(31000),
                    command: "python train.py".into(),
                }],
                cpu_history: vec![10, 50, 90, 30],
            }],
            ..Default::default()
        });
        st
    }

    #[test]
    fn visible_width_ignores_escapes() {
        assert_eq!(visible_width("\x1b[38;2;1;2;3mab\x1b[0m"), 2);
        assert_eq!(visible_width("●"), 1);
        assert_eq!(visible_width("⚠"), 1);
    }

    #[test]
    fn status_line_fits_and_drops_from_the_right() {
        let st = state();
        let wide = strip_ansi(&status_line(&st, 200));
        assert!(wide.contains("147845"));
        assert!(wide.contains("mywork"));
        assert!(wide.contains("1 notice"), "{wide}");
        assert!(wide.contains("monitorable"));
        for cols in [120, 80, 60, 40, 20] {
            let l = status_line(&st, cols);
            assert!(
                visible_width(&l) <= cols,
                "cols={cols} width={} line={l:?}",
                visible_width(&l)
            );
            assert!(l.contains("147845"), "id never dropped at {cols}");
        }
        let narrow = status_line(&st, 40);
        assert!(!narrow.contains("help"), "help hint dropped first");
        assert!(narrow.contains("left"), "remaining kept: {narrow}");
    }

    #[test]
    fn notices_and_help_modes() {
        let mut st = state();
        st.mode = BarMode::Notices { idx: 0 };
        let l = render(&st, 1, 100);
        assert!(l.contains("1/1"));
        assert!(l.contains("QUOTA over by"));
        assert!(l.contains("n next"));
        for cols in [20, 30, 60, 100] {
            let short = notices_line(&st, 0, cols);
            assert!(
                visible_width(&short) <= cols,
                "cols={cols} width={}",
                visible_width(&short)
            );
        }
        st.mode = BarMode::Help { page: 0 };
        let h = render(&st, 1, 100);
        assert!(h.contains("notices"));
        assert!(h.contains("(1/1)"));
    }

    #[test]
    fn panel_renders_selector_bars_and_procs() {
        let mut st = state();
        st.panel_open = true;
        let out = render(&st, 12, 100);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() > 5 && lines.len() <= 12, "{}", lines.len());
        assert_eq!(visible_width(lines[0]), 100, "rule spans the pane");
        assert!(lines[2].contains("c3gpu-a5-u1") && lines[2].contains("1/1"));
        assert!(lines[3].contains("cpu") && lines[3].contains("34%"));
        assert!(lines[4].contains("mem") && lines[4].contains("37%"));
        assert!(lines[5].contains("gpu0") && lines[5].contains("87%"));
        assert!(out.contains("python train.py"));
        assert!(!lines[1].contains("monitorable"), "hint hidden while open");
    }

    #[test]
    fn the_rule_tops_the_sint_region() {
        let st = state();
        // Two-row bar: rule, then the line.
        let bar = render(&st, 2, 80);
        let lines: Vec<&str> = bar.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(strip_ansi(lines[0]), "\u{2501}".repeat(80));
        assert!(lines[1].contains("147845"));
        // A one-row bar has no room for it.
        assert!(!render(&st, 1, 80).contains('\u{2501}'));
        // The panel pane draws it instead, and keeps its own rows.
        let panel = render_panel(&st, 13, 80);
        let plines: Vec<&str> = panel.lines().collect();
        assert_eq!(strip_ansi(plines[0]), "\u{2501}".repeat(80));
        assert!(plines.len() <= 13);
        assert!(plines[1].contains("c3gpu-a5-u1"));
    }

    #[test]
    fn hints_name_the_prefix_because_ctrl_b_is_one_shot() {
        let mut st = state();
        st.mode = BarMode::Notices { idx: 0 };
        let n = strip_ansi(&render(&st, 1, 100));
        assert!(n.contains("^b n next"), "{n}");
        assert!(n.contains("^b esc back"), "{n}");
        st.mode = BarMode::Status;
        st.panel_open = true;
        let p = strip_ansi(&render_panel(&st, 13, 100));
        assert!(p.contains("^b ,/. host"), "{p}");
        assert!(p.contains("^b m close"), "{p}");
    }

    #[test]
    fn spinner_and_shimmer_are_stable_width() {
        let c = DARK;
        for f in 0..20 {
            assert_eq!(visible_width(&glyph(Severity::Red, f, &c)), 1);
            assert_eq!(
                visible_width(&shimmer("1 notice", f, c.err, [c.accent, c.warn, c.accent])),
                8
            );
        }
        assert_eq!(sparkline(&[0, 50, 100], 10).chars().count(), 3);
        assert_eq!(visible_width(&bar(50, 10, c.ok, c.track)), 10);
    }
}
