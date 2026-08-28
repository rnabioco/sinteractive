//! Per-process rows from `/proc`, scoped to the job's pids or the user's
//! processes.
//!
//! CPU% needs two samples: the sampler keeps `(utime+stime, when)` per pid
//! and the second call reports `delta_ticks / delta_secs / CLK_TCK * 100`.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::time::Instant;

use super::Proc;

/// How many characters of the command line are kept.
pub const COMMAND_MAX: usize = 200;

/// How many rows a snapshot keeps.
pub const PROCS_MAX: usize = 40;

/// Previous `(cpu ticks, when)` per pid.
pub type PrevTicks = HashMap<u32, (u64, Instant)>;

/// `/etc/passwd` → uid → login name.
pub fn parse_passwd(text: &str) -> HashMap<u32, String> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            f.next()?; // password
            let uid: u32 = f.next()?.parse().ok()?;
            Some((uid, name.to_string()))
        })
        .collect()
}

pub fn read_passwd() -> HashMap<u32, String> {
    fs::read_to_string("/etc/passwd")
        .map(|s| parse_passwd(&s))
        .unwrap_or_default()
}

/// The login name for `uid`: the cache, else NSS (`getpwuid_r`, which
/// reaches LDAP/SSSD users that `/etc/passwd` does not list), else the
/// number. The answer is cached either way.
pub fn user_name(users: &mut HashMap<u32, String>, uid: u32) -> String {
    if let Some(name) = users.get(&uid) {
        return name.clone();
    }
    let name = getpwuid_name(uid).unwrap_or_else(|| uid.to_string());
    users.insert(uid, name.clone());
    name
}

fn getpwuid_name(uid: u32) -> Option<String> {
    let mut buf = vec![0u8; 16 * 1024];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: every pointer is to live, correctly sized storage; the
    // buffer length is passed alongside it.
    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || pwd.pw_name.is_null() {
        return None;
    }
    // SAFETY: pw_name points into `buf`, which is still alive.
    let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
    Some(name.to_string_lossy().into_owned())
}

/// The display command: the joined `cmdline` truncated to [`COMMAND_MAX`],
/// or `[comm]` for a kernel thread or zombie with an empty cmdline.
pub fn command_of(cmdline: &[String], comm: &str) -> String {
    if cmdline.is_empty() {
        return format!("[{comm}]");
    }
    let joined = cmdline.join(" ");
    if joined.chars().count() <= COMMAND_MAX {
        return joined;
    }
    let mut s: String = joined.chars().take(COMMAND_MAX - 1).collect();
    s.push('…');
    s
}

/// Per-process CPU% from two tick readings.
pub fn proc_cpu_pct(prev_ticks: u64, cur_ticks: u64, elapsed_secs: f64, ticks_per_sec: u64) -> f32 {
    if elapsed_secs <= 0.0 || ticks_per_sec == 0 {
        return 0.0;
    }
    let delta = cur_ticks.saturating_sub(prev_ticks) as f64;
    (delta / elapsed_secs / ticks_per_sec as f64 * 100.0).max(0.0) as f32
}

/// Which processes a snapshot covers.
pub enum ProcFilter<'a> {
    /// Exactly these pids (the job cgroup).
    Pids(&'a BTreeSet<u32>),
    /// Every process owned by this uid.
    Uid(u32),
}

/// Sample the processes selected by `filter`. `prev` is updated in place so
/// the next call can compute CPU%; entries for vanished pids are dropped.
/// Sorted by CPU% descending, then RSS, capped at [`PROCS_MAX`].
pub fn sample(
    filter: &ProcFilter<'_>,
    prev: &mut PrevTicks,
    users: &mut HashMap<u32, String>,
) -> Vec<Proc> {
    let now = Instant::now();
    let tps = procfs::ticks_per_second().max(1);
    let page_kb = procfs::page_size().max(1) / 1024;
    let mut out = Vec::new();
    let mut seen = HashMap::new();

    let mut visit = |p: procfs::process::Process| {
        let Ok(stat) = p.stat() else { return };
        let pid = stat.pid.max(0) as u32;
        let uid = p.uid().unwrap_or(u32::MAX);
        if let ProcFilter::Uid(want) = filter {
            if uid != *want {
                return;
            }
        }
        let ticks = stat.utime + stat.stime;
        let cpu_pct = match prev.get(&pid) {
            Some((t, when)) => {
                proc_cpu_pct(*t, ticks, now.duration_since(*when).as_secs_f64(), tps)
            }
            None => 0.0,
        };
        seen.insert(pid, (ticks, now));
        let cmdline = p.cmdline().unwrap_or_default();
        out.push(Proc {
            pid,
            user: user_name(users, uid),
            cpu_pct,
            rss_mb: stat.rss * page_kb / 1024,
            threads: stat.num_threads.max(0) as u32,
            state: stat.state,
            command: command_of(&cmdline, &stat.comm),
            gpu_mem_mb: None,
        });
    };

    match filter {
        ProcFilter::Pids(pids) => {
            for &pid in pids.iter() {
                if let Ok(p) = procfs::process::Process::new(pid as i32) {
                    visit(p);
                }
            }
        }
        ProcFilter::Uid(_) => {
            if let Ok(all) = procfs::process::all_processes() {
                for p in all.flatten() {
                    visit(p);
                }
            }
        }
    }

    *prev = seen;
    out.sort_by(|a, b| {
        b.cpu_pct
            .partial_cmp(&a.cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.rss_mb.cmp(&a.rss_mb))
            .then(a.pid.cmp(&b.pid))
    });
    out.truncate(PROCS_MAX);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwd_parsing() {
        let users = parse_passwd(
            "root:x:0:0:root:/root:/bin/bash\njay:x:2008414:2008414::/home/jay:/bin/zsh\nbroken line\n",
        );
        assert_eq!(users.get(&0).map(String::as_str), Some("root"));
        assert_eq!(users.get(&2008414).map(String::as_str), Some("jay"));
        assert_eq!(users.len(), 2);
    }

    #[test]
    fn user_lookup_caches_and_falls_back() {
        let mut users = HashMap::from([(7u32, "seven".to_string())]);
        assert_eq!(user_name(&mut users, 7), "seven");
        // A uid nobody has: the number, and it is remembered.
        assert_eq!(
            user_name(&mut users, u32::MAX - 1),
            (u32::MAX - 1).to_string()
        );
        assert!(users.contains_key(&(u32::MAX - 1)));
        assert_eq!(getpwuid_name(0).as_deref(), Some("root"));
    }

    #[test]
    fn command_display() {
        assert_eq!(command_of(&[], "kworker/0:1"), "[kworker/0:1]");
        assert_eq!(
            command_of(&["python".into(), "-m".into(), "x".into()], "python"),
            "python -m x"
        );
        let long = vec!["a".repeat(500)];
        let c = command_of(&long, "a");
        assert_eq!(c.chars().count(), COMMAND_MAX);
        assert!(c.ends_with('…'));
    }

    #[test]
    fn cpu_pct_math() {
        assert_eq!(proc_cpu_pct(100, 150, 1.0, 100), 50.0);
        assert_eq!(
            proc_cpu_pct(100, 500, 2.0, 100),
            200.0,
            "multi-threaded > 100"
        );
        assert_eq!(proc_cpu_pct(100, 50, 1.0, 100), 0.0, "never negative");
        assert_eq!(proc_cpu_pct(0, 1, 0.0, 100), 0.0);
    }

    #[test]
    fn samples_this_process_by_uid_and_by_pid() {
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        let me = std::process::id();
        let mut users = HashMap::new();
        let mut prev = PrevTicks::new();
        let rows = sample(&ProcFilter::Uid(uid), &mut prev, &mut users);
        let mine = rows
            .iter()
            .find(|p| p.pid == me)
            .expect("this process is listed");
        // NSS knows this uid on any sane host; the number is the last resort.
        let expected = getpwuid_name(uid).unwrap_or_else(|| uid.to_string());
        assert_eq!(mine.user, expected);
        assert_eq!(users.get(&uid), Some(&expected), "cached");
        assert!(mine.threads >= 1);
        assert!(mine.rss_mb > 0);
        assert!(prev.contains_key(&me));

        let pids: BTreeSet<u32> = [me].into_iter().collect();
        let rows = sample(&ProcFilter::Pids(&pids), &mut prev, &mut users);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pid, me);
        assert_eq!(prev.len(), 1, "vanished pids are forgotten");
    }
}
