//! Host CPU and memory: `/proc/stat`, `/proc/loadavg`, `/proc/meminfo`.
//!
//! Pure parsers over the file text; the delta arithmetic takes two parsed
//! samples so it is testable with synthetic lines.

use std::fs;

/// Aggregate jiffies from the `cpu` line of `/proc/stat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTotals {
    /// user + nice + system + irq + softirq + steal.
    pub busy: u64,
    /// busy + idle + iowait.
    pub total: u64,
    /// Number of `cpuN` lines.
    pub ncpu: u32,
}

/// Parse `/proc/stat`. `None` without an aggregate `cpu` line.
pub fn parse_proc_stat(text: &str) -> Option<CpuTotals> {
    let mut totals: Option<CpuTotals> = None;
    let mut ncpu = 0u32;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(label) = it.next() else { continue };
        if label == "cpu" {
            let v: Vec<u64> = it.map(|f| f.parse().unwrap_or(0)).collect();
            if v.len() < 4 {
                return None;
            }
            let at = |i: usize| v.get(i).copied().unwrap_or(0);
            // user nice system idle iowait irq softirq steal
            let busy = at(0) + at(1) + at(2) + at(5) + at(6) + at(7);
            let total = busy + at(3) + at(4);
            totals = Some(CpuTotals {
                busy,
                total,
                ncpu: 0,
            });
        } else if label.starts_with("cpu") && label[3..].bytes().all(|b| b.is_ascii_digit()) {
            ncpu += 1;
        }
    }
    totals.map(|t| CpuTotals { ncpu, ..t })
}

/// Busy percentage between two samples, 0–100. Zero when nothing elapsed.
pub fn cpu_pct(prev: CpuTotals, cur: CpuTotals) -> f32 {
    let total = cur.total.saturating_sub(prev.total);
    if total == 0 {
        return 0.0;
    }
    let busy = cur.busy.saturating_sub(prev.busy);
    ((busy as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
}

/// CPU time consumed by a scope (`delta_usec`) over `elapsed_secs` wall
/// time, as a percentage of `ncpu` CPUs, 0–100.
pub fn scoped_cpu_pct(delta_usec: u64, elapsed_secs: f64, ncpu: u32) -> f32 {
    if elapsed_secs <= 0.0 || ncpu == 0 {
        return 0.0;
    }
    let cpus_used = delta_usec as f64 / 1_000_000.0 / elapsed_secs;
    ((cpus_used / ncpu as f64) * 100.0).clamp(0.0, 100.0) as f32
}

/// `/proc/loadavg` → (1, 5, 15). Zeros when unparseable.
pub fn parse_loadavg(text: &str) -> (f32, f32, f32) {
    let mut it = text
        .split_whitespace()
        .map(|f| f.parse::<f32>().unwrap_or(0.0));
    let one = it.next().unwrap_or(0.0);
    let five = it.next().unwrap_or(0.0);
    let fifteen = it.next().unwrap_or(0.0);
    (one, five, fifteen)
}

/// `/proc/meminfo` → (total_mb, used_mb) with used = total − available.
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let mut total_kb = None;
    let mut avail_kb = None;
    let mut free_kb = None;
    for line in text.lines() {
        let (k, v) = match line.split_once(':') {
            Some(kv) => kv,
            None => continue,
        };
        let n: u64 = v
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        match k {
            "MemTotal" => total_kb = Some(n),
            "MemAvailable" => avail_kb = Some(n),
            "MemFree" => free_kb = Some(n),
            _ => {}
        }
    }
    let total = total_kb?;
    let avail = avail_kb.or(free_kb).unwrap_or(0);
    Some((total / 1024, total.saturating_sub(avail) / 1024))
}

/// Current `/proc/stat` totals.
pub fn read_proc_stat() -> Option<CpuTotals> {
    parse_proc_stat(&fs::read_to_string("/proc/stat").ok()?)
}

pub fn read_loadavg() -> (f32, f32, f32) {
    fs::read_to_string("/proc/loadavg")
        .map(|s| parse_loadavg(&s))
        .unwrap_or_default()
}

pub fn read_meminfo() -> Option<(u64, u64)> {
    parse_meminfo(&fs::read_to_string("/proc/meminfo").ok()?)
}

/// Online CPUs: `cpuN` lines of `/proc/stat`, else `sysconf`.
pub fn ncpu() -> u32 {
    if let Some(t) = read_proc_stat() {
        if t.ncpu > 0 {
            return t.ncpu;
        }
    }
    // SAFETY: sysconf has no preconditions.
    let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    if n > 0 {
        n as u32
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT_A: &str = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\ncpu1 50 0 25 400 25 0 0 0 0 0\nintr 1 2 3\nctxt 5\n";
    const STAT_B: &str = "cpu  250 0 100 900 50 0 0 0 0 0\ncpu0 125 0 50 450 25 0 0 0 0 0\ncpu1 125 0 50 450 25 0 0 0 0 0\n";

    #[test]
    fn proc_stat_delta_math() {
        let a = parse_proc_stat(STAT_A).expect("a");
        let b = parse_proc_stat(STAT_B).expect("b");
        assert_eq!(a.ncpu, 2);
        assert_eq!(a.busy, 150);
        assert_eq!(a.total, 1000);
        assert_eq!(b.busy, 350);
        assert_eq!(b.total, 1300);
        // 200 busy jiffies out of 300 elapsed.
        let pct = cpu_pct(a, b);
        assert!((pct - 66.666).abs() < 0.01, "{pct}");
        assert_eq!(cpu_pct(a, a), 0.0, "no time elapsed");
        assert_eq!(cpu_pct(b, a), 0.0, "counter went backwards");
        assert_eq!(parse_proc_stat("intr 1\n"), None);
    }

    #[test]
    fn scoped_cpu_pct_math() {
        // 2 CPU-seconds in 1 s on a 4-CPU allocation = 50%.
        assert_eq!(scoped_cpu_pct(2_000_000, 1.0, 4), 50.0);
        assert_eq!(scoped_cpu_pct(9_000_000, 1.0, 4), 100.0, "clamped");
        assert_eq!(scoped_cpu_pct(1, 0.0, 4), 0.0);
        assert_eq!(scoped_cpu_pct(1, 1.0, 0), 0.0);
    }

    #[test]
    fn loadavg_and_meminfo() {
        assert_eq!(
            parse_loadavg("74.69 74.50 74.10 75/3475 2028212\n"),
            (74.69, 74.5, 74.1)
        );
        assert_eq!(parse_loadavg(""), (0.0, 0.0, 0.0));
        let mi = "MemTotal:       16384000 kB\nMemFree:         1000000 kB\nMemAvailable:    8192000 kB\nBuffers:  1 kB\n";
        assert_eq!(parse_meminfo(mi), Some((16000, 8000)));
        assert_eq!(
            parse_meminfo("MemTotal: 2048 kB\nMemFree: 1024 kB\n"),
            Some((2, 1))
        );
        assert_eq!(parse_meminfo("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn reads_this_host() {
        assert!(ncpu() > 0);
        assert!(read_proc_stat().is_some());
        assert!(read_meminfo().is_some_and(|(t, _)| t > 0));
    }
}
