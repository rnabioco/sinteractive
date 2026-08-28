//! The `monitor` screen: an nvitop-style view of one host built with
//! ratatui, in the same visual family as the zellij status bar (`█░` bars,
//! block sparkline, Claude orange accent, ok/warn/err at 70/90 %).
//!
//! ```text
//! ● c3gpu-a1-u1 · job 31756988 (train) · 4 CPUs 32G gpu 0,1 · 2s ago · cache
//! gpu0 NVIDIA A100-SXM4-40GB          65°C  250/400W  1410MHz  1 proc
//!      util ██████████████████░░  87%   mem ███████████████░░░░░ 31G/40G
//! cpu  ████████░░░░░░░░░░░░  42%  of 4      ▁▂▃▅▆▇█▆▅▃▂▁
//! mem  ██████████░░░░░░░░░░  50%  8.0G / 16G
//! load 3.0 2.0 1.0 · host 64 CPUs · 12 procs
//!     PID USER        CPU%     RSS  GPU-MEM  SM%  S  COMMAND
//!    4242 jay        150.0    2.0G      30G   80  R  python train.py
//! q quit · g gpu procs only · ↑↓ scroll
//! ```
//!
//! The terminal is restored on every exit path: ratatui's init installs a
//! panic hook that restores it, and [`Restore`] does the same on drop.

use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};
use sint_core::metrics::{Proc, Snapshot};
use sint_core::theme::{Rgb, Theme};

use super::monitor::{Label, Msg};
use super::snapshot::{mb_to_g, pct_of};

/// How long the event loop waits for a key before redrawing.
const TICK: Duration = Duration::from_millis(250);

/// Widest a bar gets.
const BAR_MAX: u16 = 20;

/// Sparkline glyphs, lowest to highest.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Theme colours as ratatui colours.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    /// Values. Set explicitly on every one, because an unstyled cell takes
    /// the terminal's default foreground — inside zellij that is the theme's
    /// `fg`, a mid grey dimmer than the labels beside it.
    pub text: Color,
    pub dim: Color,
    /// The unfilled half of a gauge, below `dim`.
    pub track: Color,
    pub hint: Color,
}

fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

impl From<&Theme> for Colors {
    fn from(t: &Theme) -> Self {
        Colors {
            accent: rgb(t.accent),
            ok: rgb(t.ok),
            warn: rgb(t.warn),
            err: rgb(t.err),
            text: rgb(t.text),
            dim: rgb(t.dim),
            track: rgb(t.track),
            hint: rgb(t.hint),
        }
    }
}

impl Colors {
    /// ok < 70 ≤ warn < 90 ≤ err.
    pub fn level(&self, pct: u8) -> Color {
        if pct >= 90 {
            self.err
        } else if pct >= 70 {
            self.warn
        } else {
            self.ok
        }
    }
}

/// UI state.
pub struct App {
    pub label: Label,
    pub mode: String,
    pub colors: Colors,
    pub latest: Option<Snapshot>,
    pub waiting: Option<String>,
    pub gpu_only: bool,
    pub scroll: usize,
    pub quit: bool,
}

impl App {
    pub fn new(label: Label, mode: &str, colors: Colors) -> Self {
        App {
            label,
            mode: mode.to_string(),
            colors,
            latest: None,
            waiting: None,
            gpu_only: false,
            scroll: 0,
            quit: false,
        }
    }

    pub fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Snapshot(s) => {
                self.latest = Some(*s);
                self.waiting = None;
            }
            Msg::Waiting(w) => self.waiting = Some(w),
        }
    }

    pub fn key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Char('g') => {
                self.gpu_only = !self.gpu_only;
                self.scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll += 1,
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll += 10,
            KeyCode::Home => self.scroll = 0,
            _ => {}
        }
    }

    /// The process rows to show, honouring the GPU-only toggle.
    pub fn rows(&self) -> Vec<&Proc> {
        let Some(s) = &self.latest else {
            return Vec::new();
        };
        s.procs
            .iter()
            .filter(|p| !self.gpu_only || p.gpu_mem_mb.is_some())
            .collect()
    }
}

/// Restores the terminal when dropped.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Run the screen until `q`/Esc. `rx` delivers snapshots from the feeder.
pub fn run(rx: Receiver<Msg>, label: Label, mode: &str, theme: Theme) -> io::Result<()> {
    let mut app = App::new(label, mode, Colors::from(&theme));
    let mut terminal = ratatui::try_init()?;
    let _restore = Restore;
    event_loop(&mut terminal, &rx, &mut app)
}

fn event_loop(terminal: &mut DefaultTerminal, rx: &Receiver<Msg>, app: &mut App) -> io::Result<()> {
    // Redraw when something the screen shows can have changed: a new
    // snapshot, an input event (key, resize), or the wall-clock second the
    // "Ns ago" staleness is derived from. Everything else is the same frame
    // rebuilt, and this runs on a shared login node.
    let mut dirty = true;
    let mut drawn_second = 0i64;
    loop {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    app.apply(msg);
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    app.waiting
                        .get_or_insert_with(|| "sampler stopped".to_string());
                    dirty = true;
                    break;
                }
            }
        }
        let second = sint_core::now_epoch();
        if dirty || second != drawn_second {
            terminal.draw(|f| draw(f, app))?;
            drawn_second = second;
            dirty = false;
        }
        if event::poll(TICK)? {
            let ev = event::read()?;
            dirty = true;
            if let Event::Key(k) = ev {
                if k.kind != KeyEventKind::Release {
                    app.key(k.code, k.modifiers);
                }
            }
        }
        if app.quit {
            return Ok(());
        }
    }
}

// ---- rendering ------------------------------------------------------------

/// `█` × filled, `░` × rest.
pub fn bar<'a>(pct: u8, width: u16, col: Color, track: Color) -> Vec<Span<'a>> {
    let width = width as usize;
    let filled = (width * pct.min(100) as usize).div_ceil(100);
    vec![
        Span::styled("█".repeat(filled), Style::default().fg(col)),
        Span::styled(
            "░".repeat(width.saturating_sub(filled)),
            Style::default().fg(track),
        ),
    ]
}

/// The last `width` samples as block glyphs.
pub fn sparkline(samples: &[u8], width: u16) -> String {
    let start = samples.len().saturating_sub(width as usize);
    samples[start..]
        .iter()
        .map(|v| BLOCKS[((*v as usize).min(100) * 7) / 100])
        .collect()
}

fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let c = app.colors;
    let gpu_rows = app
        .latest
        .as_ref()
        .map(|s| s.gpus.len() as u16 * 2)
        .unwrap_or(0);
    let [header, gpus, cpu, table, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(gpu_rows),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    f.render_widget(Paragraph::new(header_line(app)), header);
    match &app.latest {
        Some(s) => {
            f.render_widget(Paragraph::new(gpu_lines(s, &c, gpus.width)), gpus);
            f.render_widget(Paragraph::new(cpu_lines(s, &c, cpu.width)), cpu);
            f.render_widget(Paragraph::new(table_lines(app, s, table)), table);
        }
        None => {
            let msg = app
                .waiting
                .clone()
                .unwrap_or_else(|| "waiting for the first snapshot…".to_string());
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(c.dim)))),
                table,
            );
        }
    }
    f.render_widget(Paragraph::new(footer_line(app)), footer);
}

fn header_line(app: &App) -> Line<'static> {
    let c = app.colors;
    let dim = Style::default().fg(c.dim);
    let host = app
        .latest
        .as_ref()
        .map(|s| s.host.clone())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| app.label.host.clone());
    let mut spans = vec![
        Span::styled("● ", Style::default().fg(c.accent)),
        Span::styled(
            host,
            Style::default().fg(c.accent).add_modifier(Modifier::BOLD),
        ),
    ];
    let job_id = app
        .label
        .job_id
        .or_else(|| app.latest.as_ref().and_then(|s| s.scope.job_id));
    if let Some(id) = job_id {
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(
            format!("job {id}"),
            Style::default().fg(c.text),
        ));
        if let Some(n) = &app.label.job_name {
            spans.push(Span::styled(format!(" ({n})"), dim));
        }
    }
    if let Some(s) = &app.latest {
        let mut alloc = Vec::new();
        if let Some(n) = s.scope.cpus_alloc {
            alloc.push(format!("{n} CPUs"));
        }
        if let Some(m) = s.scope.mem_alloc_mb {
            alloc.push(mb_to_g(m));
        }
        if let Some(g) = &s.scope.gpu_indices {
            let list: Vec<String> = g.iter().map(u32::to_string).collect();
            alloc.push(format!("gpu {}", list.join(",")));
        }
        if s.scope.job_id.is_none() {
            alloc.push("host scope".into());
        }
        if !alloc.is_empty() {
            spans.push(Span::styled(format!(" · {}", alloc.join(" ")), dim));
        }
        let age = s.age_secs(sint_core::now_epoch());
        let age_style = if age > 30 {
            Style::default().fg(c.warn)
        } else {
            dim
        };
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(format!("{age}s ago"), age_style));
    }
    spans.push(Span::styled(format!(" · {}", app.mode), dim));
    if let (Some(w), Some(_)) = (&app.waiting, &app.latest) {
        spans.push(Span::styled(
            format!("  ⚠ {w}"),
            Style::default().fg(c.warn),
        ));
    }
    Line::from(spans)
}

fn gpu_lines(s: &Snapshot, c: &Colors, width: u16) -> Vec<Line<'static>> {
    let dim = Style::default().fg(c.dim);
    let bw = bar_width(width);
    let mut out = Vec::new();
    for g in &s.gpus {
        let mut extra = Vec::new();
        if let Some(t) = g.temp_c {
            extra.push(format!("{t}°C"));
        }
        match (g.power_w, g.power_limit_w) {
            (Some(w), Some(l)) => extra.push(format!("{w}/{l}W")),
            (Some(w), None) => extra.push(format!("{w}W")),
            _ => {}
        }
        if let Some(m) = g.sm_clock_mhz {
            extra.push(format!("{m}MHz"));
        }
        let n = g.procs.len();
        if n > 0 {
            extra.push(format!("{n} proc{}", if n == 1 { "" } else { "s" }));
        }
        out.push(Line::from(vec![
            Span::styled(format!("gpu{:<2}", g.index), Style::default().fg(c.hint)),
            Span::styled(
                format!(" {:<28}", truncate(&g.name, 28)),
                Style::default().fg(c.text),
            ),
            Span::styled(format!("  {}", extra.join("  ")), dim),
        ]));
        let mem_pct = pct_of(g.mem_used_mb, g.mem_total_mb);
        let mut spans = vec![Span::styled("     util ", dim)];
        spans.extend(bar(g.util_pct, bw, c.level(g.util_pct), c.track));
        spans.push(Span::styled(
            format!(" {:>3}%", g.util_pct),
            Style::default().fg(c.level(g.util_pct)),
        ));
        spans.push(Span::styled("   mem ", dim));
        spans.extend(bar(mem_pct, bw, c.accent, c.track));
        spans.push(Span::styled(
            format!(" {}/{}", mb_to_g(g.mem_used_mb), mb_to_g(g.mem_total_mb)),
            Style::default().fg(c.text),
        ));
        out.push(Line::from(spans));
    }
    out
}

fn cpu_lines(s: &Snapshot, c: &Colors, width: u16) -> Vec<Line<'static>> {
    let dim = Style::default().fg(c.dim);
    let bw = bar_width(width);
    let cpu_pct = s.cpu.pct.round().clamp(0.0, 100.0) as u8;
    let of = s.scope.cpus_alloc.unwrap_or(s.cpu.ncpu);
    let mut cpu = vec![Span::styled("cpu  ", Style::default().fg(c.hint))];
    cpu.extend(bar(cpu_pct, bw, c.level(cpu_pct), c.track));
    cpu.push(Span::styled(
        format!(" {cpu_pct:>3}%"),
        Style::default().fg(c.level(cpu_pct)),
    ));
    cpu.push(Span::styled(format!("  of {of:<4}"), dim));
    let spark_w = width.saturating_sub(bw + 22).min(60);
    if spark_w > 0 {
        cpu.push(Span::styled(
            sparkline(&s.cpu_history, spark_w),
            Style::default().fg(c.accent),
        ));
    }

    let mem_pct = pct_of(s.mem.used_mb, s.mem.total_mb);
    let mut mem = vec![Span::styled("mem  ", Style::default().fg(c.hint))];
    mem.extend(bar(mem_pct, bw, c.level(mem_pct), c.track));
    mem.push(Span::styled(
        format!(" {mem_pct:>3}%"),
        Style::default().fg(c.level(mem_pct)),
    ));
    mem.push(Span::styled(
        format!("  {} / {}", mb_to_g(s.mem.used_mb), mb_to_g(s.mem.total_mb)),
        Style::default().fg(c.text),
    ));

    let load = Line::from(vec![
        Span::styled("load ", Style::default().fg(c.hint)),
        Span::styled(
            format!("{:.1} {:.1} {:.1}", s.cpu.load1, s.cpu.load5, s.cpu.load15),
            Style::default().fg(c.text),
        ),
        Span::styled(
            format!(
                " · host {} CPUs · {} proc{}",
                s.cpu.ncpu,
                s.procs.len(),
                if s.procs.len() == 1 { "" } else { "s" }
            ),
            dim,
        ),
    ]);
    vec![Line::from(cpu), Line::from(mem), load]
}

fn table_lines(app: &App, s: &Snapshot, area: Rect) -> Vec<Line<'static>> {
    let c = app.colors;
    let dim = Style::default().fg(c.dim);
    let rows = app.rows();
    let mut out = vec![Line::from(Span::styled(
        format!(
            "{:>7} {:<10} {:>6} {:>7} {:>8} {:>4}  {}  {}",
            "PID", "USER", "CPU%", "RSS", "GPU-MEM", "SM%", "S", "COMMAND"
        ),
        dim.add_modifier(Modifier::BOLD),
    ))];
    if rows.is_empty() {
        out.push(Line::from(Span::styled(
            if app.gpu_only {
                "no processes hold a GPU (g shows all)"
            } else {
                "no processes in scope"
            },
            dim,
        )));
        return out;
    }
    let visible = area.height.saturating_sub(1) as usize;
    let max_scroll = rows.len().saturating_sub(visible);
    let start = app.scroll.min(max_scroll);
    let cmd_room = (area.width as usize).saturating_sub(55).max(8);
    for p in rows.iter().skip(start).take(visible) {
        let cpu_c = c.level(p.cpu_pct.round().clamp(0.0, 255.0) as u8);
        let gpu_mem = p.gpu_mem_mb.map(mb_to_g).unwrap_or_else(|| "-".into());
        let sm = s
            .gpu_sm_pct(p.pid)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into());
        let gpu_style = if p.gpu_mem_mb.is_some() {
            Style::default().fg(c.accent)
        } else {
            dim
        };
        out.push(Line::from(vec![
            Span::styled(format!("{:>7} ", p.pid), Style::default().fg(c.text)),
            Span::styled(format!("{:<10} ", truncate(&p.user, 10)), dim),
            Span::styled(format!("{:>6.1} ", p.cpu_pct), Style::default().fg(cpu_c)),
            Span::styled(
                format!("{:>7} ", mb_to_g(p.rss_mb)),
                Style::default().fg(c.text),
            ),
            Span::styled(format!("{gpu_mem:>8} {sm:>4}"), gpu_style),
            Span::styled(format!("  {}  ", p.state), dim),
            Span::styled(truncate(&p.command, cmd_room), Style::default().fg(c.text)),
        ]));
    }
    if start + visible < rows.len() {
        let last = out.len() - 1;
        out[last] = Line::from(Span::styled(
            format!("… {} more (↓)", rows.len() - start - visible + 1),
            dim,
        ));
    }
    out
}

fn footer_line(app: &App) -> Line<'static> {
    let c = app.colors;
    let key = Style::default().fg(c.hint);
    let dim = Style::default().fg(c.dim);
    Line::from(vec![
        Span::styled("q", key),
        Span::styled(" quit · ", dim),
        Span::styled("g", key),
        Span::styled(
            if app.gpu_only {
                " all procs · "
            } else {
                " gpu procs only · "
            },
            dim,
        ),
        Span::styled("↑↓", key),
        Span::styled(" scroll", dim),
    ])
}

fn bar_width(width: u16) -> u16 {
    (width / 4).clamp(5, BAR_MAX)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use sint_core::metrics::{Cpu, Gpu, Mem, Scope};

    fn snap() -> Snapshot {
        Snapshot {
            host: "n1".into(),
            ts: sint_core::now_epoch(),
            scope: Scope {
                job_id: Some(7),
                cpus_alloc: Some(4),
                mem_alloc_mb: Some(16384),
                ..Default::default()
            },
            cpu: Cpu {
                pct: 42.0,
                ncpu: 64,
                load1: 3.0,
                load5: 2.0,
                load15: 1.0,
            },
            mem: Mem {
                total_mb: 16384,
                used_mb: 8192,
            },
            gpus: vec![Gpu {
                index: 0,
                name: "A100".into(),
                util_pct: 95,
                mem_used_mb: 30720,
                mem_total_mb: 40960,
                temp_c: Some(65),
                power_w: Some(250),
                power_limit_w: Some(400),
                sm_clock_mhz: Some(1410),
                procs: vec![],
            }],
            procs: (0..30)
                .map(|i| Proc {
                    pid: 100 + i,
                    user: "jay".into(),
                    cpu_pct: 100.0 - i as f32,
                    rss_mb: 1024,
                    threads: 1,
                    state: 'R',
                    command: format!("cmd{i}"),
                    gpu_mem_mb: (i % 2 == 0).then_some(1024),
                })
                .collect(),
            cpu_history: vec![0, 50, 100],
        }
    }

    fn app() -> App {
        let mut a = App::new(
            Label {
                host: "n1".into(),
                job_id: Some(7),
                job_name: Some("web".into()),
            },
            "cache",
            Colors::from(&Theme::DARK),
        );
        a.apply(Msg::Snapshot(Box::new(snap())));
        a
    }

    fn text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn bars_and_sparklines() {
        let b = bar(50, 10, Color::Red, Color::Gray);
        assert_eq!(b[0].content.as_ref(), "█████");
        assert_eq!(b[1].content.as_ref(), "░░░░░");
        let b = bar(1, 10, Color::Red, Color::Gray);
        assert_eq!(b[0].content.as_ref(), "█", "any load lights a cell");
        assert_eq!(sparkline(&[0, 50, 100], 10), "▁▄█");
        assert_eq!(sparkline(&[0, 50, 100], 2), "▄█", "keeps the newest");
        assert_eq!(bar_width(200), BAR_MAX);
        assert_eq!(bar_width(10), 5);
    }

    #[test]
    fn levels() {
        let c = Colors::from(&Theme::DARK);
        assert_eq!(c.level(69), c.ok);
        assert_eq!(c.level(70), c.warn);
        assert_eq!(c.level(90), c.err);
    }

    #[test]
    fn keys_drive_state() {
        let mut a = app();
        assert_eq!(a.rows().len(), 30);
        a.key(KeyCode::Char('g'), KeyModifiers::NONE);
        assert!(a.gpu_only);
        assert_eq!(a.rows().len(), 15);
        a.key(KeyCode::Down, KeyModifiers::NONE);
        a.key(KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(a.scroll, 11);
        a.key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(a.scroll, 10);
        a.key(KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(a.scroll, 0);
        a.key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(a.scroll, 0);
        assert!(!a.quit);
        a.key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(a.quit);
        let mut a = app();
        a.key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(a.quit);
    }

    #[test]
    fn lines_render() {
        let a = app();
        let s = a.latest.as_ref().unwrap();
        let h = text(&[header_line(&a)]);
        assert!(h.starts_with("● n1 · job 7 (web) · 4 CPUs 16G · "), "{h}");
        assert!(h.ends_with("s ago · cache"), "{h}");

        let g = text(&gpu_lines(s, &a.colors, 80));
        assert!(g.starts_with("gpu0  A100"), "{g}");
        assert!(g.contains("65°C  250/400W  1410MHz"), "{g}");
        assert!(g.contains(" 95%   mem "), "{g}");
        assert!(g.contains(" 30G/40G"), "{g}");

        let c = text(&cpu_lines(s, &a.colors, 80));
        assert!(c.contains("cpu  ") && c.contains(" 42%  of 4   ▁▄█"), "{c}");
        assert!(c.contains("mem  ") && c.contains(" 50%  8.0G / 16G"), "{c}");
        assert!(
            c.contains("load 3.0 2.0 1.0 · host 64 CPUs · 30 procs"),
            "{c}"
        );

        let t = text(&table_lines(&a, s, Rect::new(0, 0, 100, 6)));
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), 6, "header + 5 rows");
        assert!(lines[0].contains("PID") && lines[0].contains("COMMAND"));
        assert!(
            lines[1].contains("100 jay") && lines[1].contains("100.0"),
            "{}",
            lines[1]
        );
        assert!(
            lines[1].contains("1.0G") && lines[1].contains("R  cmd0"),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("       -    -"), "no gpu: {}", lines[2]);
        assert!(lines[5].starts_with("… "), "{}", lines[5]);

        // Waiting while a snapshot is shown appears in the header.
        let mut a = app();
        a.apply(Msg::Waiting("ssh: timeout".into()));
        assert!(text(&[header_line(&a)]).contains("⚠ ssh: timeout"));
        // Waiting with nothing yet keeps the label's host.
        let mut empty = App::new(
            Label {
                host: "node01".into(),
                ..Default::default()
            },
            "ssh",
            Colors::from(&Theme::LIGHT),
        );
        empty.apply(Msg::Waiting("no snapshot yet".into()));
        assert_eq!(text(&[header_line(&empty)]), "● node01 · ssh");
        let f = text(&[footer_line(&empty)]);
        assert_eq!(f, "q quit · g gpu procs only · ↑↓ scroll");
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("héllo", 10), "héllo");
        assert_eq!(truncate("héllo wörld", 6), "héllo…");
    }
}
