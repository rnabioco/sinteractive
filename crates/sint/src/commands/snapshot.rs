//! `sinteractive snapshot [--json] [--job JOBID]` — one host snapshot,
//! scoped to the Slurm job this process runs in (host-wide otherwise), or
//! to `--job`'s cgroup on this host.
//!
//! Two samples one second apart so CPU% is a real delta; the second is
//! printed. This is also what `monitor --live` runs over ssh on the node,
//! and what `__job` runs (with `--job`) on the nodes of the user's other
//! running jobs.

use std::time::Duration;

use anyhow::Result;
use sint_core::color::Palette;
use sint_core::metrics::{Sampler, Scope, Snapshot};

use super::common::{print_json, Ctx};
use crate::cli::SnapshotArgs;

/// How far apart the two samples are.
pub const SAMPLE_GAP: Duration = Duration::from_secs(1);

/// Take the two-sample snapshot of `scope`.
pub fn take(scope: Scope) -> (Snapshot, Option<String>) {
    let mut sampler = Sampler::new(scope);
    sampler.sample();
    std::thread::sleep(SAMPLE_GAP);
    let snap = sampler.sample();
    let gpu_error = sampler.gpu_error().map(str::to_string);
    (snap, gpu_error)
}

pub fn run(args: SnapshotArgs) -> Result<i32> {
    let scope = match args.job {
        Some(id) => Scope::for_job(id),
        None => Scope::for_current_job(),
    };
    let (snap, gpu_error) = take(scope);
    if args.json {
        print_json(&snap)?;
        return Ok(0);
    }
    let ctx = Ctx::new();
    print!(
        "{}",
        render_human(&snap, gpu_error.as_deref(), &ctx.palette(1))
    );
    Ok(0)
}

/// `1.5G` / `12G` from MB.
pub fn mb_to_g(mb: u64) -> String {
    let g = mb as f64 / 1024.0;
    if g >= 10.0 {
        format!("{g:.0}G")
    } else {
        format!("{g:.1}G")
    }
}

/// `used` as a percentage of `total`, clamped to 100; 0 when `total` is 0.
pub fn pct_of(used: u64, total: u64) -> u8 {
    (used * 100).checked_div(total).unwrap_or(0).min(100) as u8
}

/// The compact human dump: header, cpu/mem lines, one line per GPU, the
/// top 10 processes.
pub fn render_human(snap: &Snapshot, gpu_error: Option<&str>, p: &Palette) -> String {
    let (reset, bold, dim, id, key) = (&p.reset, &p.bold, &p.dim, &p.id, &p.key);
    let mut out = String::new();

    let mut head = format!("{bold}{id}{}{reset}", snap.host);
    let sc = &snap.scope;
    match sc.job_id {
        Some(job) => head.push_str(&format!(" {dim}·{reset} job {id}{job}{reset}")),
        None => head.push_str(&format!(" {dim}· host scope{reset}")),
    }
    if let Some(cg) = &sc.cgroup {
        head.push_str(&format!(" {dim}· cgroup {cg}{reset}"));
    }
    let mut alloc = Vec::new();
    if let Some(c) = sc.cpus_alloc {
        alloc.push(format!("{c} CPUs"));
    }
    if let Some(m) = sc.mem_alloc_mb {
        alloc.push(mb_to_g(m));
    }
    if let Some(g) = &sc.gpu_indices {
        let list: Vec<String> = g.iter().map(u32::to_string).collect();
        alloc.push(format!("gpu {}", list.join(",")));
    }
    if !alloc.is_empty() {
        head.push_str(&format!(" {dim}· {}{reset}", alloc.join(" ")));
    }
    out.push_str(&head);
    out.push('\n');

    let cpu_c = level_colour(snap.cpu.pct.round() as u8, p);
    let of = sc.cpus_alloc.unwrap_or(snap.cpu.ncpu);
    out.push_str(&format!(
        "  {key}{:<5}{reset} {cpu_c}{:>3.0}%{reset} {dim}of {of} · load {:.1} {:.1} {:.1} · host {} CPUs{reset}\n",
        "cpu", snap.cpu.pct, snap.cpu.load1, snap.cpu.load5, snap.cpu.load15, snap.cpu.ncpu
    ));
    let mem_pct = pct_of(snap.mem.used_mb, snap.mem.total_mb);
    let mem_c = level_colour(mem_pct, p);
    out.push_str(&format!(
        "  {key}{:<5}{reset} {mem_c}{:>3}%{reset} {dim}{} / {}{reset}\n",
        "mem",
        mem_pct,
        mb_to_g(snap.mem.used_mb),
        mb_to_g(snap.mem.total_mb)
    ));
    if snap.gpus.is_empty() {
        let why = gpu_error.unwrap_or("none visible");
        out.push_str(&format!("  {key}{:<5}{reset} {dim}{why}{reset}\n", "gpu"));
    }
    for g in &snap.gpus {
        let util_c = level_colour(g.util_pct, p);
        let mut extra = Vec::new();
        if let Some(t) = g.temp_c {
            extra.push(format!("{t}°C"));
        }
        match (g.power_w, g.power_limit_w) {
            (Some(w), Some(l)) => extra.push(format!("{w}/{l}W")),
            (Some(w), None) => extra.push(format!("{w}W")),
            _ => {}
        }
        if let Some(c) = g.sm_clock_mhz {
            extra.push(format!("{c}MHz"));
        }
        if !g.procs.is_empty() {
            extra.push(format!("{} procs", g.procs.len()));
        }
        out.push_str(&format!(
            "  {key}gpu{:<2}{reset} {util_c}{:>3}%{reset} {dim}{} / {} · {}{}{reset}\n",
            g.index,
            g.util_pct,
            mb_to_g(g.mem_used_mb),
            mb_to_g(g.mem_total_mb),
            g.name,
            if extra.is_empty() {
                String::new()
            } else {
                format!(" · {}", extra.join(" "))
            }
        ));
    }
    if !snap.procs.is_empty() {
        out.push_str(&format!(
            "  {dim}{:>7} {:<10} {:>6} {:>6} {:>7} {:>4}  {}{reset}\n",
            "PID", "USER", "CPU%", "RSS", "GPU-MEM", "SM%", "COMMAND"
        ));
        for pr in snap.procs.iter().take(10) {
            let gpu_mem = pr.gpu_mem_mb.map(mb_to_g).unwrap_or_else(|| "-".into());
            let sm = snap
                .gpu_sm_pct(pr.pid)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into());
            let cmd: String = pr.command.chars().take(60).collect();
            let user: String = pr.user.chars().take(10).collect();
            out.push_str(&format!(
                "  {:>7} {:<10} {:>6.1} {:>6} {:>7} {:>4}  {cmd}\n",
                pr.pid,
                user,
                pr.cpu_pct,
                mb_to_g(pr.rss_mb),
                gpu_mem,
                sm
            ));
        }
    }
    out
}

/// ok < 70 ≤ warn < 90 ≤ err.
fn level_colour(pct: u8, p: &Palette) -> &str {
    if pct >= 90 {
        &p.err
    } else if pct >= 70 {
        &p.warn
    } else {
        &p.ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sint_core::metrics::{Cpu, Gpu, Mem, Proc, Scope};

    #[test]
    fn human_dump_without_colour() {
        let snap = Snapshot {
            host: "n1".into(),
            ts: 1,
            scope: Scope {
                job_id: Some(7),
                cgroup: Some("slurm/uid_1/job_7".into()),
                cpus_alloc: Some(4),
                mem_alloc_mb: Some(16384),
                gpu_indices: Some(vec![0, 1]),
            },
            cpu: Cpu {
                pct: 42.4,
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
                util_pct: 87,
                mem_used_mb: 30720,
                mem_total_mb: 40960,
                temp_c: Some(65),
                power_w: Some(250),
                power_limit_w: Some(400),
                sm_clock_mhz: Some(1410),
                procs: vec![],
            }],
            procs: vec![Proc {
                pid: 4242,
                user: "jay".into(),
                cpu_pct: 150.0,
                rss_mb: 2048,
                threads: 8,
                state: 'R',
                command: "python train.py".into(),
                gpu_mem_mb: Some(30720),
            }],
            cpu_history: vec![],
        };
        let s = render_human(&snap, None, &Palette::none());
        assert!(s.starts_with("n1 · job 7 · cgroup slurm/uid_1/job_7 · 4 CPUs 16G gpu 0,1\n"));
        assert!(
            s.contains("  cpu    42% of 4 · load 3.0 2.0 1.0 · host 64 CPUs\n"),
            "{s}"
        );
        assert!(s.contains("  mem    50% 8.0G / 16G\n"), "{s}");
        assert!(
            s.contains("  gpu0   87% 30G / 40G · A100 · 65°C 250/400W 1410MHz\n"),
            "{s}"
        );
        assert!(
            s.contains("   4242 jay         150.0   2.0G     30G    -  python train.py\n"),
            "{s}"
        );

        let none = Snapshot::default();
        let s = render_human(&none, Some("no driver"), &Palette::none());
        assert!(s.contains(" · host scope\n"));
        assert!(s.contains("  gpu   no driver\n"), "{s}");
        assert!(!s.contains("PID"));
    }

    #[test]
    fn units() {
        assert_eq!(mb_to_g(512), "0.5G");
        assert_eq!(mb_to_g(16384), "16G");
        assert_eq!(pct_of(1, 0), 0);
        assert_eq!(pct_of(3, 2), 100);
    }
}
