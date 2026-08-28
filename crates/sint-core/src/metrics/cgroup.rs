//! The Slurm job cgroup: where a job's processes, CPU time and memory live
//! on a shared node.
//!
//! Slurm (`proctrack/cgroup` + `jobacct_gather/cgroup`) puts every step of
//! a job under one directory per job. Two layouts exist:
//!
//! - **cgroup v2** (unified): `/sys/fs/cgroup/system.slice/slurmstepd.scope/job_<id>/`
//!   with `cgroup.procs`, `cpu.max`, `cpu.stat`, `cpuset.cpus.effective`,
//!   `memory.max`, `memory.current`. Older releases used other parents, so a
//!   bounded search for `**/job_<id>` is the fallback.
//! - **cgroup v1** (per-controller, what Alpine's RHEL 8 nodes run):
//!   `/sys/fs/cgroup/<controller>/slurm/uid_<uid>/job_<id>/` for `cpuset`,
//!   `memory`, `cpu,cpuacct` (and `freezer`, `devices`). The job directory's
//!   own `cgroup.procs` is empty — pids sit in `step_<name>/task_<n>/` below
//!   it — so pid collection walks the subtree.
//!
//! Every reader is a pure parser over the file's text plus a thin
//! `read_to_string`, so the arithmetic is unit-tested without a cgroup.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Where cgroups are mounted.
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// How deep the fallback `**/job_<id>` search and the pid walk go.
const MAX_DEPTH: usize = 6;

/// The v1 "no limit" value (`LONG_MAX` rounded down to a page) and anything
/// near it: memory limits at or above this mean "unlimited".
const V1_UNLIMITED: u64 = 1 << 60;

/// The directories that make up one job's cgroup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobCgroup {
    /// Path relative to the cgroup root, for display: `slurm/uid_1000/job_7`
    /// or `system.slice/slurmstepd.scope/job_7`.
    pub name: String,
    /// v2 unified directory.
    pub unified: Option<PathBuf>,
    /// v1 `cpuset` controller directory.
    pub cpuset: Option<PathBuf>,
    /// v1 `memory` controller directory.
    pub memory: Option<PathBuf>,
    /// v1 `cpu,cpuacct` (or `cpuacct`) controller directory.
    pub cpuacct: Option<PathBuf>,
}

impl JobCgroup {
    /// Locate job `job_id` owned by `uid` under `root` (normally
    /// [`CGROUP_ROOT`]). `None` when neither layout has it.
    pub fn find(root: &Path, job_id: u64, uid: u32) -> Option<JobCgroup> {
        let leaf = format!("job_{job_id}");
        if root.join("cgroup.controllers").is_file() {
            let candidates = [
                root.join("system.slice")
                    .join("slurmstepd.scope")
                    .join(&leaf),
                root.join("slurm").join(format!("uid_{uid}")).join(&leaf),
            ];
            let dir = candidates
                .into_iter()
                .find(|p| p.is_dir())
                .or_else(|| find_dir_named(root, &leaf, MAX_DEPTH))?;
            let name = relative_name(root, &dir);
            return Some(JobCgroup {
                name,
                unified: Some(dir),
                ..Default::default()
            });
        }
        let rel = Path::new("slurm").join(format!("uid_{uid}")).join(&leaf);
        let controller = |names: &[&str]| -> Option<PathBuf> {
            names
                .iter()
                .map(|c| root.join(c).join(&rel))
                .find(|p| p.is_dir())
                .or_else(|| {
                    names
                        .iter()
                        .filter(|c| root.join(c).is_dir())
                        .find_map(|c| find_dir_named(&root.join(c), &leaf, MAX_DEPTH))
                })
        };
        let cpuset = controller(&["cpuset"]);
        let memory = controller(&["memory"]);
        let cpuacct = controller(&["cpu,cpuacct", "cpuacct", "cpu"]);
        let any = cpuset.as_ref().or(memory.as_ref()).or(cpuacct.as_ref())?;
        // Strip the controller component so the name reads the same as v2.
        let name = relative_name(root, any)
            .split_once('/')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default();
        Some(JobCgroup {
            name,
            unified: None,
            cpuset,
            memory,
            cpuacct,
        })
    }

    /// Every pid in the job: the union of `cgroup.procs` in the job
    /// directory and all its step/task children.
    pub fn pids(&self) -> BTreeSet<u32> {
        let dir = self
            .unified
            .as_ref()
            .or(self.memory.as_ref())
            .or(self.cpuset.as_ref())
            .or(self.cpuacct.as_ref());
        let mut out = BTreeSet::new();
        if let Some(dir) = dir {
            collect_pids(dir, MAX_DEPTH, &mut out);
        }
        out
    }

    /// CPUs the job may use: the CFS quota when one is set, else the cpuset.
    pub fn cpus_alloc(&self) -> Option<u32> {
        if let Some(dir) = &self.unified {
            if let Some(q) = read(dir, "cpu.max").and_then(|s| parse_cpu_max(&s)) {
                return Some(q);
            }
            return read(dir, "cpuset.cpus.effective")
                .and_then(|s| parse_cpuset(&s))
                .or_else(|| read(dir, "cpuset.cpus").and_then(|s| parse_cpuset(&s)));
        }
        if let Some(dir) = &self.cpuacct {
            let quota = read(dir, "cpu.cfs_quota_us").and_then(|s| s.trim().parse::<i64>().ok());
            let period = read(dir, "cpu.cfs_period_us").and_then(|s| s.trim().parse::<i64>().ok());
            if let Some(q) = cfs_cpus(quota, period) {
                return Some(q);
            }
        }
        let dir = self.cpuset.as_ref()?;
        read(dir, "cpuset.cpus")
            .and_then(|s| parse_cpuset(&s))
            .or_else(|| read(dir, "cpuset.effective_cpus").and_then(|s| parse_cpuset(&s)))
    }

    /// The memory limit in MB; `None` when unlimited or unknown.
    pub fn mem_limit_mb(&self) -> Option<u64> {
        let bytes = match (&self.unified, &self.memory) {
            (Some(dir), _) => read(dir, "memory.max").and_then(|s| parse_memory_max(&s))?,
            (None, Some(dir)) => {
                read(dir, "memory.limit_in_bytes").and_then(|s| parse_memory_max(&s))?
            }
            _ => return None,
        };
        Some(bytes / (1024 * 1024))
    }

    /// Memory in use in MB. v1 `usage_in_bytes` counts reclaimable page
    /// cache, so `total_inactive_file` from `memory.stat` is subtracted as
    /// container runtimes do; v2 `memory.current` is reported as is.
    pub fn mem_used_mb(&self) -> Option<u64> {
        let bytes = match (&self.unified, &self.memory) {
            (Some(dir), _) => read(dir, "memory.current")?.trim().parse::<u64>().ok()?,
            (None, Some(dir)) => {
                let usage = read(dir, "memory.usage_in_bytes")?
                    .trim()
                    .parse::<u64>()
                    .ok()?;
                let inactive = read(dir, "memory.stat")
                    .and_then(|s| stat_field(&s, "total_inactive_file"))
                    .unwrap_or(0);
                usage.saturating_sub(inactive)
            }
            _ => return None,
        };
        Some(bytes / (1024 * 1024))
    }

    /// Cumulative CPU time of the job in microseconds.
    pub fn cpu_usage_usec(&self) -> Option<u64> {
        if let Some(dir) = &self.unified {
            return read(dir, "cpu.stat").and_then(|s| stat_field(&s, "usage_usec"));
        }
        let dir = self.cpuacct.as_ref()?;
        let ns = read(dir, "cpuacct.usage")?.trim().parse::<u64>().ok()?;
        Some(ns / 1000)
    }
}

/// The job id of the calling process, from `/proc/self/cgroup` text: the
/// first `job_<id>` path component on any line.
pub fn job_id_from_proc_cgroup(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let path = line.rsplit(':').next()?;
        path.split('/')
            .find_map(|c| c.strip_prefix("job_").and_then(|id| id.parse().ok()))
    })
}

/// `0-3,8` → 5; `17-18,38` → 3; empty → `None`.
pub fn parse_cpuset(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut n = 0u32;
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('-') {
            Some((a, b)) => {
                let a: u32 = a.trim().parse().ok()?;
                let b: u32 = b.trim().parse().ok()?;
                n += b.checked_sub(a)? + 1;
            }
            None => {
                part.parse::<u32>().ok()?;
                n += 1;
            }
        }
    }
    (n > 0).then_some(n)
}

/// v2 `cpu.max`: `"max 100000"` → `None`, `"200000 100000"` → 2 (rounded
/// up, at least 1).
pub fn parse_cpu_max(s: &str) -> Option<u32> {
    let mut it = s.split_whitespace();
    let quota = it.next()?;
    if quota == "max" {
        return None;
    }
    let quota: i64 = quota.parse().ok()?;
    let period: i64 = it.next().unwrap_or("100000").parse().ok()?;
    cfs_cpus(Some(quota), Some(period))
}

/// CFS quota/period → CPUs (rounded up). `None` for unlimited (`-1`, `0`).
pub fn cfs_cpus(quota: Option<i64>, period: Option<i64>) -> Option<u32> {
    let quota = quota?;
    let period = period.unwrap_or(100_000);
    if quota <= 0 || period <= 0 {
        return None;
    }
    Some(((quota + period - 1) / period).max(1) as u32)
}

/// `memory.max` / `memory.limit_in_bytes`: bytes, or `None` for `max` and
/// the v1 "no limit" sentinel.
pub fn parse_memory_max(s: &str) -> Option<u64> {
    let s = s.trim();
    if s == "max" {
        return None;
    }
    let v: u64 = s.parse().ok()?;
    (v < V1_UNLIMITED).then_some(v)
}

/// One `key value` line of a `*.stat` file.
pub fn stat_field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(' ')?;
        (k == key).then(|| v.trim().parse().ok()).flatten()
    })
}

/// `cgroup.procs` text → pids (one per line, junk skipped).
pub fn parse_pids(text: &str) -> Vec<u32> {
    text.lines().filter_map(|l| l.trim().parse().ok()).collect()
}

fn read(dir: &Path, file: &str) -> Option<String> {
    fs::read_to_string(dir.join(file)).ok()
}

fn relative_name(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root)
        .unwrap_or(dir)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string()
}

/// Depth-bounded search for a directory named `leaf`.
fn find_dir_named(root: &Path, leaf: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = fs::read_dir(root).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        // Symlinked controllers (`cpu` → `cpu,cpuacct`) would double the walk.
        let Ok(ft) = e.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|n| n == leaf) {
            return Some(path);
        }
        subdirs.push(path);
    }
    subdirs
        .iter()
        .find_map(|d| find_dir_named(d, leaf, depth - 1))
}

fn collect_pids(dir: &Path, depth: usize, out: &mut BTreeSet<u32>) {
    if let Some(text) = read(dir, "cgroup.procs") {
        out.extend(parse_pids(&text));
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            collect_pids(&e.path(), depth - 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpuset_ranges() {
        assert_eq!(parse_cpuset("0-3,8"), Some(5));
        assert_eq!(parse_cpuset("17-18,38\n"), Some(3));
        assert_eq!(parse_cpuset("7"), Some(1));
        assert_eq!(parse_cpuset("0-63"), Some(64));
        assert_eq!(parse_cpuset(""), None);
        assert_eq!(parse_cpuset("\n"), None);
        assert_eq!(parse_cpuset("3-1"), None, "inverted range");
        assert_eq!(parse_cpuset("a-b"), None);
    }

    #[test]
    fn cpu_max_and_cfs() {
        assert_eq!(parse_cpu_max("max 100000"), None);
        assert_eq!(parse_cpu_max("200000 100000"), Some(2));
        assert_eq!(parse_cpu_max("150000 100000"), Some(2), "rounds up");
        assert_eq!(parse_cpu_max("50000 100000"), Some(1), "at least one");
        assert_eq!(cfs_cpus(Some(-1), Some(100000)), None);
        assert_eq!(cfs_cpus(None, Some(100000)), None);
        assert_eq!(cfs_cpus(Some(400000), None), Some(4));
    }

    #[test]
    fn memory_max_sentinels() {
        assert_eq!(parse_memory_max("max\n"), None);
        assert_eq!(parse_memory_max("8589934592\n"), Some(8589934592));
        assert_eq!(parse_memory_max("9223372036854771712"), None, "v1 LONG_MAX");
        assert_eq!(parse_memory_max("junk"), None);
    }

    #[test]
    fn stat_fields_and_pids() {
        let stat = "usage_usec 482602\nuser_usec 400000\nsystem_usec 82602\n";
        assert_eq!(stat_field(stat, "usage_usec"), Some(482602));
        assert_eq!(stat_field(stat, "nope"), None);
        assert_eq!(
            parse_pids("1170405\n1170410\n\nx\n"),
            vec![1170405, 1170410]
        );
    }

    #[test]
    fn job_id_from_proc_self_cgroup_v1_and_v2() {
        let v1 = "12:hugetlb:/\n10:freezer:/slurm/uid_2008414/job_31756988/step_batch\n\
                  9:memory:/slurm/uid_2008414/job_31756988/step_batch/task_0\n";
        assert_eq!(job_id_from_proc_cgroup(v1), Some(31756988));
        let v2 = "0::/system.slice/slurmstepd.scope/job_42/step_0/user/task_0\n";
        assert_eq!(job_id_from_proc_cgroup(v2), Some(42));
        assert_eq!(
            job_id_from_proc_cgroup("0::/user.slice/user-1.slice\n"),
            None
        );
    }

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    #[test]
    fn v1_layout_like_alpine() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job = Path::new("slurm/uid_2008414/job_31756988");
        let cpuset = root.join("cpuset").join(job);
        let memory = root.join("memory").join(job);
        let cpuacct = root.join("cpu,cpuacct").join(job);
        write(&cpuset.join("cgroup.procs"), "");
        write(&cpuset.join("cpuset.cpus"), "17-18,38\n");
        write(&memory.join("cgroup.procs"), "");
        write(&memory.join("memory.limit_in_bytes"), "8589934592\n");
        write(&memory.join("memory.usage_in_bytes"), "1130430464\n");
        write(
            &memory.join("memory.stat"),
            "cache 0\ntotal_rss 500000000\ntotal_inactive_file 104857600\n",
        );
        write(&memory.join("step_batch/cgroup.procs"), "1170405\n");
        write(
            &memory.join("step_batch/task_0/cgroup.procs"),
            "1170410\n1170411\n",
        );
        write(&memory.join("step_extern/task_0/cgroup.procs"), "1170300\n");
        write(&cpuacct.join("cgroup.procs"), "");
        write(&cpuacct.join("cpu.cfs_quota_us"), "-1\n");
        write(&cpuacct.join("cpu.cfs_period_us"), "100000\n");
        write(&cpuacct.join("cpuacct.usage"), "482602773465\n");

        let cg = JobCgroup::find(root, 31756988, 2008414).expect("found");
        assert_eq!(cg.name, "slurm/uid_2008414/job_31756988");
        assert_eq!(cg.unified, None);
        assert_eq!(cg.cpuset.as_deref(), Some(cpuset.as_path()));
        assert_eq!(cg.memory.as_deref(), Some(memory.as_path()));
        assert_eq!(cg.cpuacct.as_deref(), Some(cpuacct.as_path()));
        assert_eq!(cg.cpus_alloc(), Some(3), "no CFS quota → cpuset");
        assert_eq!(cg.mem_limit_mb(), Some(8192));
        assert_eq!(cg.mem_used_mb(), Some((1130430464 - 104857600) / 1048576));
        assert_eq!(cg.cpu_usage_usec(), Some(482602773));
        assert_eq!(
            cg.pids().into_iter().collect::<Vec<_>>(),
            vec![1170300, 1170405, 1170410, 1170411]
        );

        assert_eq!(JobCgroup::find(root, 1, 2008414), None);
        // A wrong uid guess still lands on the job via the bounded search:
        // job ids are unique cluster-wide, so the uid is only a shortcut.
        let by_search = JobCgroup::find(root, 31756988, 7).expect("found by search");
        assert_eq!(by_search.name, "slurm/uid_2008414/job_31756988");
    }

    #[test]
    fn v2_layout_with_fallback_search() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("cgroup.controllers"), "cpuset cpu memory\n");
        let dir = root.join("system.slice/slurmstepd.scope/job_42");
        write(&dir.join("cgroup.procs"), "10\n");
        write(&dir.join("step_0/user/task_0/cgroup.procs"), "11\n12\n");
        write(&dir.join("cpu.max"), "max 100000\n");
        write(&dir.join("cpuset.cpus.effective"), "0-3\n");
        write(&dir.join("memory.max"), "4294967296\n");
        write(&dir.join("memory.current"), "1073741824\n");
        write(&dir.join("cpu.stat"), "usage_usec 12345\nuser_usec 1\n");

        let cg = JobCgroup::find(root, 42, 1000).expect("found");
        assert_eq!(cg.name, "system.slice/slurmstepd.scope/job_42");
        assert_eq!(cg.unified.as_deref(), Some(dir.as_path()));
        assert_eq!(cg.cpus_alloc(), Some(4));
        assert_eq!(cg.mem_limit_mb(), Some(4096));
        assert_eq!(cg.mem_used_mb(), Some(1024));
        assert_eq!(cg.cpu_usage_usec(), Some(12345));
        assert_eq!(cg.pids().into_iter().collect::<Vec<_>>(), vec![10, 11, 12]);

        // A quota beats the cpuset.
        write(&dir.join("cpu.max"), "250000 100000\n");
        assert_eq!(cg.cpus_alloc(), Some(3));

        // Unknown parent → bounded search finds it.
        let odd = root.join("machine.slice/slurm/job_43");
        write(&odd.join("cgroup.procs"), "99\n");
        write(&odd.join("memory.max"), "max\n");
        let cg = JobCgroup::find(root, 43, 1000).expect("found by search");
        assert_eq!(cg.name, "machine.slice/slurm/job_43");
        assert_eq!(cg.mem_limit_mb(), None);
        assert_eq!(cg.cpus_alloc(), None);
    }
}
