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
    /// The fence rule: the accent lifted towards white. It runs the full
    /// width of the pane, so at the accent's own weight it pulled the eye
    /// away from the words under it; a lighter orange still fences the
    /// region off without competing with it.
    pub rule: (u8, u8, u8),
}

pub const DARK: Colors = Colors {
    accent: (0xD9, 0x77, 0x57),
    ok: (0x4E, 0xBA, 0x65),
    warn: (0xFF, 0xC1, 0x07),
    err: (0xFF, 0x6B, 0x80),
    text: (0xFF, 0xFF, 0xFF),
    dim: (0xD9, 0xD9, 0xD9),
    track: (0x59, 0x59, 0x59),
    hint: (0x8C, 0x9E, 0xFF),
    rule: (0xE6, 0xA6, 0x92),
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
    rule: (0xDF, 0x8B, 0x70),
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

/// The bar line's left inset. The rule spans the pane edge to edge, but the
/// words sit one column in: hard against the terminal's left edge they read
/// as if they had been clipped.
const PAD: &str = " ";

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
    format!("{}{}{RESET}", fg(c.rule), "\u{2501}".repeat(cols))
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
            text: format!(
                "{}{}{RESET} {}launched{RESET}",
                fg(c.text),
                m.jobs,
                fg(c.dim)
            ),
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
    // The way out is named the same way everywhere: `close`. `next` only
    // when there is a next; on the last notice `^b n` closes as well.
    let hint = if idx + 1 < n {
        format!("{}^b n next · ^b esc close{RESET}", fg(c.dim))
    } else {
        format!("{}^b n or ^b esc close{RESET}", fg(c.dim))
    };
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
    let sep = format!(" {}·{RESET} ", fg(c.dim));
    // `(1/2)` said there was a second page but not how to reach it: the key
    // that opens help is the key that pages it, and nothing on the bar said
    // so. The counter is dropped for a lone page, and the nav hint is the
    // first thing to go on a narrow bar — the keys themselves are the point.
    let counter = if pages > 1 {
        format!("({}/{pages}) ", page + 1)
    } else {
        String::new()
    };
    let nav = if page + 1 < pages {
        "^b h more · ^b esc close"
    } else {
        "^b esc close"
    };
    let tail = |nav: &str| match format!("{counter}{nav}") {
        t if t.is_empty() => String::new(),
        t => format!("   {}{}{RESET}", fg(c.dim), t.trim_end()),
    };
    let mut tail_s = tail(nav);
    let mut line = parts.join(&sep);
    if visible_width(&line) + visible_width(&tail_s) > cols {
        tail_s = tail("");
    }
    if visible_width(&line) + visible_width(&tail_s) > cols {
        // Trim entries from the end until it fits.
        while parts.len() > 2 && visible_width(&parts.join(&sep)) + visible_width(&tail_s) > cols {
            parts.pop();
        }
        line = parts.join(&sep);
    }
    format!("{line}{tail_s}")
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

/// `key desc`, the panel's legend unit.
fn keycap(k: &str, desc: &str, c: &Colors) -> String {
    format!("{}{k}{RESET} {}{desc}{RESET}", fg(c.hint), fg(c.dim))
}

/// `left`, then `right` pushed to the far edge. The right side is dropped
/// whole rather than truncated when the row is too narrow for both.
fn spread(left: &str, right: &str, cols: usize) -> String {
    let (lw, rw) = (visible_width(left), visible_width(right));
    if rw > 0 && lw + rw + 2 <= cols {
        format!("{left}{}{right}", " ".repeat(cols - lw - rw))
    } else {
        left.to_string()
    }
}

/// How a job reads in the strip: the id, plus its name when it has one.
fn job_label(h: &sint_proto::HostPanel) -> String {
    match h.job_name.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => format!("{} {n}", h.job_id),
        None => h.job_id.to_string(),
    }
}

/// The job strip: every monitorable job on one row, the selected one lit,
/// with the keys that drive the panel on the right. When the jobs do not
/// all fit, the strip scrolls to keep the selected one in view and marks
/// the ends it has cut with `‹` / `›`.
pub fn job_strip(st: &State, cols: usize) -> String {
    let c = colors(st.theme);
    let hosts = &st.msg.hosts;
    let n = hosts.len();
    // Focused, the panel owns the bare keys; unfocused, only chords reach it.
    let legend: Vec<String> = if st.focused {
        vec![
            keycap("←→", "job", &c),
            keycap("t", "top", &c),
            keycap("esc", "shell", &c),
            keycap("x", "close", &c),
        ]
    } else {
        vec![keycap("^b m", "focus", &c), keycap("^b ,/.", "job", &c)]
    };
    let legend = legend.join(&format!(" {}·{RESET} ", fg(c.dim)));
    let count = format!("{}{}/{n}{RESET}", fg(c.dim), st.host_idx + 1);
    let room = cols.saturating_sub(visible_width(&legend) + visible_width(&count) + 6);

    // Grow a window around the selection: right first, then left. A label
    // wider than the whole strip is cut, so one long job name cannot push
    // the row past the pane and wrap it.
    let labels: Vec<String> = hosts
        .iter()
        .map(|h| {
            let l = job_label(h);
            if visible_width(&l) > room && room > 1 {
                let mut t: String = l.chars().take(room - 1).collect();
                t.push('…');
                t
            } else {
                l
            }
        })
        .collect();
    let sep_w = 3; // " · "
    let (mut start, mut end) = (st.host_idx, st.host_idx + 1);
    let mut used = visible_width(&labels[st.host_idx]);
    loop {
        let grew_right = end < n && used + sep_w + visible_width(&labels[end]) <= room;
        if grew_right {
            used += sep_w + visible_width(&labels[end]);
            end += 1;
        }
        let grew_left = start > 0 && used + sep_w + visible_width(&labels[start - 1]) <= room;
        if grew_left {
            start -= 1;
            used += sep_w + visible_width(&labels[start]);
        }
        if !grew_right && !grew_left {
            break;
        }
    }
    let shown: Vec<String> = (start..end)
        .map(|i| {
            if i == st.host_idx {
                format!("{}{BOLD}{}{RESET}", fg(c.accent), labels[i])
            } else {
                format!("{}{}{RESET}", fg(c.dim), labels[i])
            }
        })
        .collect();
    let mut left = shown.join(&format!(" {}·{RESET} ", fg(c.dim)));
    if start > 0 {
        left = format!("{}‹{RESET} {left}", fg(c.hint));
    }
    if end < n {
        left = format!("{left} {}›{RESET}", fg(c.hint));
    }
    spread(&format!("{left}  {count}"), &legend, cols)
}

/// The monitor panel rows (excluding the accent rule), `rows` high.
///
/// Bars only: the job strip, cpu, memory, and as many GPUs as there is room
/// for, then the cpu trend if any row is still going spare. The process
/// table is not here — `t` opens the full `sinteractive monitor` TUI in a
/// floating pane, which scrolls and sorts the way a top does.
///
/// A wide pane lays the resources out two to a row — cpu beside mem, then
/// the GPUs in pairs — so a four-GPU node fits its five rows with one to
/// spare instead of losing gpu2 and gpu3 off the bottom. The threshold is
/// the width at which both halves still get a bar worth reading
/// (`MIN_WIDE_BAR` cells); below it the rows stack as before.
pub fn panel_lines(st: &State, rows: usize, cols: usize) -> Vec<String> {
    let c = colors(st.theme);
    let mut out = Vec::new();
    if rows == 0 {
        return out;
    }
    let Some(h) = st.msg.hosts.get(st.host_idx) else {
        out.push(format!(
            "{}no monitorable jobs — start one and it appears here{RESET}",
            fg(c.dim)
        ));
        return out;
    };
    out.push(job_strip(st, cols));
    let per_row = if cols >= 2 * (WIDEST_LABEL + 1 + MIN_WIDE_BAR + ROW_TAIL) + GUTTER {
        2
    } else {
        1
    };
    // cpu and mem come first; the GPUs get whatever rows are left after
    // them, `per_row` to each.
    let gpu_slots = rows.saturating_sub(out.len() + 2usize.div_ceil(per_row)) * per_row;
    let shown = &h.gpus[..h.gpus.len().min(gpu_slots)];
    // The resource rows share one grid — label, bar, percentage, then the
    // row's own detail — so a `gpu0` under `cpu` and `mem` does not set its
    // bar one cell to the right of theirs. The label column is as wide as
    // the widest label that can turn up (`trend`, or a two-digit GPU), not
    // the widest on this host, so the grid stays put as the selection moves
    // between a CPU job and a GPU job. Both halves of a paired row use the
    // same grid, so mem's slash sits under gpu1's as it does under gpu0's
    // when the rows stack.
    let lw = shown
        .iter()
        .map(|g| format!("gpu{}", g.index).len())
        .fold(WIDEST_LABEL, usize::max);
    let cw = if per_row == 2 {
        (cols - GUTTER) / 2
    } else {
        cols
    };
    let bw = MAX_BAR.min(cw.saturating_sub(lw + 1 + ROW_TAIL).max(MIN_BAR));
    let label = |s: &str| format!("{}{s:<lw$}{RESET}", fg(c.dim));
    let pct = |p: u8| format!("{}{p:>3}%{RESET}", fg(c.text));
    // `used / total`, the amount right-aligned and the total padded on the
    // right when something follows it, so the slashes and what comes after
    // line up between the memory row and the GPU rows.
    let used_of = |used: u64, total: u64, tw: usize| {
        format!(
            "{}{:>4}{RESET} {}/{RESET} {}{:<tw$}{RESET}",
            fg(c.text),
            mb_to_g(used),
            fg(c.dim),
            fg(c.text),
            mb_to_g(total)
        )
    };
    let mut cells = vec![format!(
        "{} {} {}  {}of{RESET} {}{}{RESET} {}·{RESET} {}load{RESET} {}{:.1}{RESET}",
        label("cpu"),
        bar(h.cpu_pct, bw, c.ok, c.track),
        pct(h.cpu_pct),
        fg(c.dim),
        fg(c.text),
        h.cpu_alloc,
        fg(c.dim),
        fg(c.dim),
        fg(c.text),
        h.load1,
    )];
    let mem_pct = pct_of(h.mem_used_mb, h.mem_alloc_mb);
    cells.push(format!(
        "{} {} {}  {}",
        label("mem"),
        bar(mem_pct, bw, c.accent, c.track),
        pct(mem_pct),
        used_of(h.mem_used_mb, h.mem_alloc_mb, 0)
    ));
    for g in shown {
        let mem_pct = pct_of(g.mem_used_mb, g.mem_total_mb);
        let extra = [
            g.temp_c.map(|t| format!("{t}°C")),
            g.power_w.map(|p| format!("{p}W")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        cells.push(format!(
            "{} {} {}  {}  {}{:<10}{RESET} {}",
            label(&format!("gpu{}", g.index)),
            bar(g.util_pct, bw, c.warn, c.track),
            pct(g.util_pct),
            used_of(g.mem_used_mb, g.mem_total_mb, 4),
            fg(c.text),
            extra,
            bar(mem_pct, GPU_MEM_BAR, c.accent, c.track)
        ));
    }
    out.extend(cells.chunks(per_row).map(|pair| beside(pair, cw)));
    // The sample's age rides on the first resource row: the cpu row when
    // they stack, `cpu | mem` when they pair — either way the row with the
    // least of its own to say.
    if h.age_secs > 30 {
        out[1].push_str(&format!("  {}{}s old{RESET}", fg(c.warn), h.age_secs));
    }
    // A CPU-only job leaves rows over: spend them on where the load has
    // been, which is the one thing a bar cannot say. The sparkline is the
    // bars' width and sits under them, one more row of the same column,
    // rather than running out past the rows above it to wherever the
    // history happens to end.
    if out.len() < rows && h.cpu_history.len() > 1 {
        out.push(format!(
            "{} {}",
            label("trend"),
            sparkline(&h.cpu_history, bw)
        ));
    }
    out.truncate(rows);
    out
}

/// One or two resource cells as a row: the first padded to `cw`, then the
/// gutter, then the second — so a bar in the right column starts in the
/// same place on every row.
fn beside(cells: &[String], cw: usize) -> String {
    match cells {
        [left, right] => {
            let pad = cw.saturating_sub(visible_width(left)) + GUTTER;
            format!("{left}{}{right}", " ".repeat(pad))
        }
        [one] => one.clone(),
        _ => String::new(),
    }
}

/// Widest label a resource row can carry: `trend`, or a two-digit GPU.
const WIDEST_LABEL: usize = "trend".len();

/// Everything after a resource row's bar, at its widest — the GPU row's
/// ` 100%  31G / 40G  61°C 240W ` and its memory mini-bar.
const ROW_TAIL: usize = 34 + GPU_MEM_BAR;

/// The bar a resource row gets when there is room, and the least it is ever
/// cut to when there is not.
const MAX_BAR: usize = 24;
const MIN_BAR: usize = 5;

/// The bar each half of a paired row must be able to hold before the panel
/// pairs rows at all: any thinner and the bars stop being readable, so a
/// pane narrower than that stacks them and shows fewer GPUs instead.
const MIN_WIDE_BAR: usize = 12;

/// Width of the memory mini-bar at the end of a GPU row.
const GPU_MEM_BAR: usize = 8;

/// Columns between the two halves of a paired row.
const GUTTER: usize = 2;

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
    // The inset is the bar's, not each mode's: every mode line is built for
    // the columns left after it.
    let inner = cols.saturating_sub(PAD.len());
    let line = match st.mode {
        BarMode::Status => status_line(st, inner),
        BarMode::Notices { idx } => notices_line(st, idx, inner),
        BarMode::Help { page } => help_line(st, page, inner),
    };
    // Row 0 is the rule whenever the bar has room for it. The bar is two rows
    // in both layouts, so with the panel open the region reads as a framed
    // block: rule, panel, rule, status line. (In the single-pane fallback the
    // bar draws the panel itself, below the line.)
    let mut out: Vec<String> = Vec::new();
    if rows > 1 {
        out.push(rule(cols, &colors(st.theme)));
    }
    out.push(format!("{PAD}{line}"));
    if st.panel_open && rows > out.len() {
        out.extend(panel_lines(st, rows - out.len(), cols));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PANEL_ROWS;
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
    fn the_bar_line_is_inset_from_the_left_edge() {
        let mut st = state();
        for mode in [
            BarMode::Status,
            BarMode::Notices { idx: 0 },
            BarMode::Help { page: 0 },
        ] {
            st.mode = mode;
            for cols in [40usize, 80, 200] {
                let line = render(&st, 1, cols);
                assert!(line.starts_with(' '), "{mode:?} at {cols}: {line:?}");
                assert!(
                    visible_width(&line) <= cols,
                    "{mode:?} at {cols} width={}",
                    visible_width(&line)
                );
            }
        }
    }

    #[test]
    fn notices_and_help_modes() {
        let mut st = state();
        st.mode = BarMode::Notices { idx: 0 };
        let l = render(&st, 1, 100);
        assert!(l.contains("1/1"));
        assert!(l.contains("QUOTA over by"));
        // The only notice: no "next" to promise, both keys close.
        assert!(l.contains("^b n or ^b esc close"), "{l}");
        let mut two = st.msg.clone();
        two.notices.push(two.notices[0].clone());
        st.apply_msg(two);
        let l = render(&st, 1, 100);
        assert!(
            l.contains("1/2") && l.contains("^b n next · ^b esc close"),
            "{l}"
        );
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
        // One page: no counter to explain, just the way out.
        assert!(!h.contains("(1/1)"), "{h}");
        assert!(h.contains("^b esc close"), "{h}");
    }

    #[test]
    fn the_help_counter_names_the_key_that_turns_the_page() {
        let mut st = state();
        let mut msg = st.msg.clone();
        msg.help.push(vec![("c".into(), "new pane".into())]);
        st.apply_msg(msg);
        st.mode = BarMode::Help { page: 0 };
        let first = strip_ansi(&render(&st, 1, 100));
        assert!(first.contains("(1/2) ^b h more"), "{first}");
        st.mode = BarMode::Help { page: 1 };
        let last = strip_ansi(&render(&st, 1, 100));
        // The last page has nowhere to page on to; `^b h` closes, as does esc.
        assert!(last.contains("(2/2) ^b esc close"), "{last}");
        // The hint goes before the keys do when the bar is narrow.
        for cols in [24usize, 40, 60] {
            let narrow = help_line(&st, 1, cols);
            assert!(
                visible_width(&narrow) <= cols,
                "cols={cols} width={}",
                visible_width(&narrow)
            );
        }
    }

    #[test]
    fn the_panel_is_bars_and_a_job_strip() {
        let mut st = state();
        st.is_panel = true;
        st.panel_open = true;
        let out = render_panel(&st, PANEL_ROWS + 1, 100);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), PANEL_ROWS + 1, "{out}");
        assert_eq!(visible_width(lines[0]), 100, "rule spans the pane");
        assert!(lines[1].contains("147845") && lines[1].contains("mywork"));
        assert!(lines[1].contains("1/1"));
        assert!(lines[2].contains("cpu") && lines[2].contains("34%"));
        assert!(lines[3].contains("mem") && lines[3].contains("37%"));
        assert!(lines[4].contains("gpu0") && lines[4].contains("87%"));
        assert!(lines[5].contains("trend"), "spare row shows the history");
        assert!(!out.contains("python train.py"), "top lives behind `t`");
        for l in &lines {
            assert!(visible_width(l) <= 100, "{l:?}");
        }
    }

    #[test]
    fn resource_rows_share_a_grid() {
        let mut st = state();
        st.is_panel = true;
        st.panel_open = true;
        let col = |l: &str, pred: fn(char) -> bool| l.chars().position(pred).unwrap();
        let bar_col = |l: &str| col(l, |ch| ch == '█' || ch == '░');
        let pct_col = |l: &str| col(l, |ch| ch == '%');
        let slash_col = |l: &str| col(l, |ch| ch == '/');

        // A GPU host: cpu, mem and gpu0 start their bars and land their
        // percentages in the same column; mem's slash sits under gpu0's.
        let gpu: Vec<String> = panel_lines(&st, PANEL_ROWS, 100)
            .iter()
            .map(|l| strip_ansi(l))
            .collect();
        let (cpu, mem, gpu0) = (&gpu[1], &gpu[2], &gpu[3]);
        assert!(gpu0.starts_with("gpu0"), "{gpu0:?}");
        assert_eq!(bar_col(cpu), bar_col(mem), "{cpu:?} {mem:?}");
        assert_eq!(bar_col(cpu), bar_col(gpu0), "{cpu:?} {gpu0:?}");
        assert_eq!(pct_col(cpu), pct_col(gpu0), "{cpu:?} {gpu0:?}");
        assert_eq!(slash_col(mem), slash_col(gpu0), "{mem:?} {gpu0:?}");

        // A CPU-only host keeps the same grid, so the bars do not jump when
        // the selection moves between the two; the spare row is the trend.
        let mut m = st.msg.clone();
        m.hosts[0].gpus.clear();
        st.apply_msg(m);
        let cpu_only: Vec<String> = panel_lines(&st, PANEL_ROWS, 100)
            .iter()
            .map(|l| strip_ansi(l))
            .collect();
        assert_eq!(bar_col(&cpu_only[1]), bar_col(cpu), "{:?}", cpu_only[1]);
        assert!(cpu_only[3].starts_with("trend"), "{:?}", cpu_only[3]);
        assert_eq!(
            col(&cpu_only[3], |ch| ch == '▁'),
            bar_col(cpu),
            "the trend starts under the bars: {:?}",
            cpu_only[3]
        );
        let mut m = st.msg.clone();
        m.hosts[0].cpu_history = (0..60).map(|i| i % 100).collect();
        st.apply_msg(m);
        let trend = strip_ansi(&panel_lines(&st, PANEL_ROWS, 100)[3]);
        assert_eq!(
            visible_width(&trend),
            bar_col(cpu) + MAX_BAR,
            "the trend ends with the bars, however long the history: {trend:?}"
        );

        // Narrower panes still fit: the bar gives way first. (Below ~53
        // columns the fixed text alone is wider than the pane, as before.)
        for cols in [80, 60] {
            for l in panel_lines(&st, PANEL_ROWS, cols) {
                assert!(visible_width(&l) <= cols, "cols={cols} {l:?}");
            }
        }
    }

    #[test]
    fn a_wide_pane_pairs_the_rows_so_four_gpus_fit() {
        let mut st = state();
        st.is_panel = true;
        st.panel_open = true;
        let mut m = st.msg.clone();
        let g0 = m.hosts[0].gpus[0].clone();
        m.hosts[0].gpus = (0..4)
            .map(|i| GpuLine {
                index: i,
                util_pct: 20 * (i as u8 + 1),
                ..g0.clone()
            })
            .collect();
        st.apply_msg(m);
        let plain = |st: &State, cols: usize| -> Vec<String> {
            panel_lines(st, PANEL_ROWS, cols)
                .iter()
                .map(|l| strip_ansi(l))
                .collect()
        };

        // Stacked, the panel runs out of rows at gpu1 and has no trend.
        let narrow = plain(&st, 100);
        assert_eq!(narrow.len(), PANEL_ROWS);
        assert!(narrow[3].starts_with("gpu0") && narrow[4].starts_with("gpu1"));
        assert!(!narrow.iter().any(|l| l.contains("gpu2")), "{narrow:?}");

        // Paired, all four fit with a row left for the trend, and the right
        // column is its own grid: mem, gpu1 and gpu3 start in one place.
        let wide = plain(&st, 160);
        assert_eq!(wide.len(), PANEL_ROWS, "{wide:?}");
        assert!(
            wide[1].starts_with("cpu") && wide[1].contains("mem"),
            "{:?}",
            wide[1]
        );
        assert!(
            wide[2].starts_with("gpu0") && wide[2].contains("gpu1"),
            "{:?}",
            wide[2]
        );
        assert!(
            wide[3].starts_with("gpu2") && wide[3].contains("gpu3"),
            "{:?}",
            wide[3]
        );
        assert!(wide[4].starts_with("trend"), "{:?}", wide[4]);
        let right = |l: &str, label: &str| l[..l.find(label).unwrap()].chars().count();
        assert_eq!(right(&wide[1], "mem"), right(&wide[2], "gpu1"));
        assert_eq!(right(&wide[1], "mem"), right(&wide[3], "gpu3"));
        let bars = |l: &str| l.match_indices(['█', '░']).count();
        assert_eq!(bars(&wide[1]), 2 * MAX_BAR, "two full bars: {:?}", wide[1]);
        for l in &wide {
            assert!(visible_width(l) <= 160, "{l:?}");
        }

        // An odd GPU leaves the right half of its row empty, not padded.
        let mut m = st.msg.clone();
        m.hosts[0].gpus.truncate(3);
        st.apply_msg(m);
        let odd = plain(&st, 160);
        assert!(
            odd[3].starts_with("gpu2") && !odd[3].ends_with(' '),
            "{:?}",
            odd[3]
        );
        assert!(odd[4].starts_with("trend"), "{:?}", odd[4]);

        // At the threshold the bars are the narrowest worth pairing; one
        // column short of it the rows stack again, with room for a fat bar.
        let at = 2 * (WIDEST_LABEL + 1 + MIN_WIDE_BAR + ROW_TAIL) + GUTTER;
        let edge = plain(&st, at);
        assert!(edge[2].contains("gpu1"), "{:?}", edge[2]);
        assert_eq!(
            bars(&edge[2]) - 2 * GPU_MEM_BAR,
            2 * MIN_WIDE_BAR,
            "{:?}",
            edge[2]
        );
        for l in &edge {
            assert!(visible_width(l) <= at, "{l:?}");
        }
        assert_eq!(
            visible_width(&edge[4]) - WIDEST_LABEL - 1,
            MIN_WIDE_BAR.min(st.msg.hosts[0].cpu_history.len()),
            "the trend shrinks with the bars: {:?}",
            edge[4]
        );
        let under = plain(&st, at - 1);
        assert!(
            under[3].starts_with("gpu0") && !under[2].contains("gpu"),
            "{under:?}"
        );
        assert_eq!(bars(&under[1]), MAX_BAR, "{:?}", under[1]);
    }

    #[test]
    fn the_strip_scrolls_to_keep_the_selection_in_view() {
        let mut st = state();
        st.is_panel = true;
        let mut m = st.msg.clone();
        m.hosts = (0..8)
            .map(|i| HostPanel {
                host: format!("c3cpu-a{i}"),
                job_id: 200 + i,
                job_name: Some(format!("job-number-{i}")),
                ..Default::default()
            })
            .collect();
        st.apply_msg(m);
        st.host_idx = 7;
        let row = job_strip(&st, 80);
        assert!(visible_width(&row) <= 80, "{}", visible_width(&row));
        let plain = strip_ansi(&row);
        assert!(plain.contains("207 job-number-7"), "{plain}");
        assert!(plain.contains('‹') && !plain.contains('›'), "{plain}");
        assert!(plain.contains("8/8"));
        st.host_idx = 0;
        let plain = strip_ansi(&job_strip(&st, 80));
        assert!(plain.contains('›') && !plain.contains('‹'), "{plain}");
        // Every job fits on a wide bar, and none of it is cut.
        let plain = strip_ansi(&job_strip(&st, 200));
        assert!(!plain.contains('‹') && !plain.contains('›'), "{plain}");
        assert!(plain.contains("200 job-number-0") && plain.contains("207 job-number-7"));
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
        let panel = render_panel(&st, PANEL_ROWS + 1, 80);
        let plines: Vec<&str> = panel.lines().collect();
        assert_eq!(strip_ansi(plines[0]), "\u{2501}".repeat(80));
        assert!(plines.len() <= PANEL_ROWS + 1);
        assert!(plines[1].contains("147845"));
    }

    #[test]
    fn hints_name_the_prefix_because_ctrl_b_is_one_shot() {
        let mut st = state();
        st.mode = BarMode::Notices { idx: 0 };
        let n = strip_ansi(&render(&st, 1, 100));
        assert!(n.contains("^b n or ^b esc close"), "{n}");
        st.mode = BarMode::Status;
        st.is_panel = true;
        st.panel_open = true;
        // Unfocused, the panel can only be reached by a chord …
        let p = strip_ansi(&render_panel(&st, PANEL_ROWS + 1, 100));
        assert!(p.contains("^b m focus"), "{p}");
        assert!(p.contains("^b ,/. job"), "{p}");
        // … focused, it owns the bare keys and says so.
        st.focused = true;
        let f = strip_ansi(&render_panel(&st, PANEL_ROWS + 1, 100));
        assert!(f.contains("←→ job"), "{f}");
        assert!(
            f.contains("t top") && f.contains("esc shell") && f.contains("x close"),
            "{f}"
        );
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
