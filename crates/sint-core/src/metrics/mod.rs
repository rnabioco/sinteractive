//! Host monitoring: one [`Snapshot`] of CPU, memory, GPUs and processes,
//! nvitop-style, scoped to the Slurm job on shared nodes.
//!
//! - [`cgroup`] — the job's cgroup (pids, CPU time, memory limit/usage)
//! - [`cpu`]    — `/proc/stat`, `/proc/loadavg`, `/proc/meminfo` parsers
//! - [`procs`]  — per-process rows with two-sample CPU%
//! - [`gpu`]    — NVML, loaded lazily; empty without a driver
//!
//! A [`Sampler`] holds the between-sample state (previous counters, the
//! NVML handle, the CPU history ring). Call [`Sampler::sample`] at ≥ 1 s
//! intervals; the first call reports 0% CPU because it has nothing to
//! diff against. The in-session loop (`__job`) writes each snapshot to
//! `<jobid>.metrics.json` with [`write_snapshot`] so `monitor` on a login
//! node can read it without ssh; [`Snapshot::to_host_panel`] is the plugin's
//! view of the same data.

pub mod cgroup;
pub mod cpu;
pub mod gpu;
pub mod procs;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::state::{atomic_write, StateDir};
use cgroup::JobCgroup;
use cpu::CpuTotals;
use procs::{PrevTicks, ProcFilter};

/// Samples kept for the CPU sparkline.
pub const HISTORY_LEN: usize = 60;

/// Snapshots older than this are "no snapshot yet" to readers.
pub const STALE_AFTER_SECS: i64 = 30;

/// One observation of a host, scoped to a job when inside one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Snapshot {
    pub host: String,
    /// Epoch seconds when taken.
    pub ts: i64,
    pub scope: Scope,
    pub cpu: Cpu,
    pub mem: Mem,
    pub gpus: Vec<Gpu>,
    pub procs: Vec<Proc>,
    /// Last [`HISTORY_LEN`] CPU% samples, oldest first.
    pub cpu_history: Vec<u8>,
}

/// What the snapshot is scoped to: a job's allocation or the whole host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Scope {
    pub job_id: Option<u64>,
    /// Cgroup path relative to `/sys/fs/cgroup`.
    pub cgroup: Option<String>,
    pub cpus_alloc: Option<u32>,
    pub mem_alloc_mb: Option<u64>,
    /// GPU indices visible to the job; `None` = every GPU on the host.
    pub gpu_indices: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Cpu {
    /// Busy percentage of the scope's CPUs (allocation when scoped, host
    /// otherwise), 0–100.
    pub pct: f32,
    /// CPUs on the host.
    pub ncpu: u32,
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,
}

/// Memory: the cgroup's usage/limit inside a job, else the host's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Mem {
    pub total_mb: u64,
    pub used_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Gpu {
    pub index: u32,
    pub name: String,
    pub util_pct: u8,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub temp_c: Option<u32>,
    pub power_w: Option<u32>,
    pub power_limit_w: Option<u32>,
    pub sm_clock_mhz: Option<u32>,
    pub procs: Vec<GpuProc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct GpuProc {
    pub pid: u32,
    pub mem_mb: u64,
    /// SM utilisation attributed to the process, when the driver reports it.
    pub sm_pct: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Proc {
    pub pid: u32,
    pub user: String,
    /// May exceed 100 for multi-threaded processes.
    pub cpu_pct: f32,
    pub rss_mb: u64,
    pub threads: u32,
    /// `/proc/<pid>/stat` state letter (`R`, `S`, `D`, `Z`, …).
    pub state: char,
    pub command: String,
    /// GPU memory held across all GPUs, when any.
    pub gpu_mem_mb: Option<u64>,
}

impl Default for Proc {
    fn default() -> Self {
        Proc {
            pid: 0,
            user: String::new(),
            cpu_pct: 0.0,
            rss_mb: 0,
            threads: 0,
            state: '?',
            command: String::new(),
            gpu_mem_mb: None,
        }
    }
}

impl Scope {
    /// The scope of the calling process: the Slurm job from
    /// `SLURM_JOB_ID`, `SINTERACTIVE_JOB_ID` or `/proc/self/cgroup`, its
    /// cgroup's limits, and the GPU indices Slurm/CUDA export. Host scope
    /// when not inside a job.
    pub fn for_current_job() -> Scope {
        let job_id = ["SLURM_JOB_ID", "SINTERACTIVE_JOB_ID"]
            .iter()
            .find_map(|k| std::env::var(k).ok()?.trim().parse::<u64>().ok())
            .or_else(|| {
                std::fs::read_to_string("/proc/self/cgroup")
                    .ok()
                    .and_then(|t| cgroup::job_id_from_proc_cgroup(&t))
            });
        let mut scope = Scope {
            job_id,
            gpu_indices: gpu::indices_from_env(),
            ..Default::default()
        };
        if let Some(cg) =
            job_id.and_then(|id| JobCgroup::find(Path::new(cgroup::CGROUP_ROOT), id, uid()))
        {
            scope.cgroup = Some(cg.name.clone());
            scope.cpus_alloc = cg.cpus_alloc();
            scope.mem_alloc_mb = cg.mem_limit_mb();
        }
        if scope.cpus_alloc.is_none() {
            scope.cpus_alloc = ["SLURM_CPUS_ON_NODE", "SLURM_CPUS_PER_TASK"]
                .iter()
                .find_map(|k| std::env::var(k).ok()?.trim().parse().ok());
        }
        if scope.mem_alloc_mb.is_none() {
            scope.mem_alloc_mb = std::env::var("SLURM_MEM_PER_NODE")
                .ok()
                .and_then(|v| v.trim().parse().ok());
        }
        scope
    }
}

/// Between-sample state. One per process; sample at ≥ 1 s intervals.
pub struct Sampler {
    scope: Scope,
    cgroup: Option<JobCgroup>,
    uid: u32,
    host: String,
    ncpu: u32,
    users: HashMap<u32, String>,
    prev_host: Option<CpuTotals>,
    prev_cgroup: Option<(u64, Instant)>,
    prev_procs: PrevTicks,
    gpu: gpu::GpuSampler,
    history: VecDeque<u8>,
}

impl Sampler {
    /// A sampler for `scope`. When the scope names a job whose cgroup is
    /// present on this host, CPU, memory and processes come from it.
    pub fn new(scope: Scope) -> Self {
        Self::with_cgroup_root(scope, Path::new(cgroup::CGROUP_ROOT))
    }

    /// The calling process's job (see [`Scope::for_current_job`]).
    pub fn for_current_job() -> Self {
        Self::new(Scope::for_current_job())
    }

    /// [`Sampler::new`] with the cgroup mount point made explicit (tests).
    pub fn with_cgroup_root(scope: Scope, root: &Path) -> Self {
        let uid = uid();
        let cgroup = scope.job_id.and_then(|id| JobCgroup::find(root, id, uid));
        Sampler {
            scope,
            cgroup,
            uid,
            host: hostname(),
            ncpu: cpu::ncpu(),
            users: procs::read_passwd(),
            prev_host: None,
            prev_cgroup: None,
            prev_procs: PrevTicks::new(),
            gpu: gpu::GpuSampler::new(),
            history: VecDeque::with_capacity(HISTORY_LEN),
        }
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Whether the job cgroup was found (scoped sampling is in effect).
    pub fn scoped(&self) -> bool {
        self.cgroup.is_some()
    }

    /// Why GPUs are absent, when NVML failed to load.
    pub fn gpu_error(&self) -> Option<&str> {
        self.gpu.last_error.as_deref()
    }

    /// Take a snapshot. CPU% is 0 on the first call.
    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let (load1, load5, load15) = cpu::read_loadavg();

        // CPU: the cgroup's CPU time over its allocation, else the host.
        let host_now = cpu::read_proc_stat();
        let mut pct = 0.0f32;
        let mut scoped_cpu = false;
        if let Some(cg) = &self.cgroup {
            if let Some(usec) = cg.cpu_usage_usec() {
                scoped_cpu = true;
                if let Some((prev, when)) = self.prev_cgroup {
                    let ncpu = self.scope.cpus_alloc.unwrap_or(self.ncpu);
                    pct = cpu::scoped_cpu_pct(
                        usec.saturating_sub(prev),
                        now.duration_since(when).as_secs_f64(),
                        ncpu,
                    );
                }
                self.prev_cgroup = Some((usec, now));
            }
        }
        if !scoped_cpu {
            if let (Some(prev), Some(cur)) = (self.prev_host, host_now) {
                pct = cpu::cpu_pct(prev, cur);
            }
        }
        self.prev_host = host_now;

        // Memory: cgroup usage against its limit, else the host.
        let host_mem = cpu::read_meminfo().unwrap_or((0, 0));
        let mem = match &self.cgroup {
            Some(cg) => Mem {
                total_mb: cg
                    .mem_limit_mb()
                    .or(self.scope.mem_alloc_mb)
                    .unwrap_or(host_mem.0),
                used_mb: cg.mem_used_mb().unwrap_or(host_mem.1),
            },
            None => Mem {
                total_mb: self.scope.mem_alloc_mb.unwrap_or(host_mem.0),
                used_mb: host_mem.1,
            },
        };

        // Processes: the cgroup's pids, else everything of this uid.
        let pids: Option<BTreeSet<u32>> = self.cgroup.as_ref().map(|cg| cg.pids());
        let filter = match &pids {
            Some(p) => ProcFilter::Pids(p),
            None => ProcFilter::Uid(self.uid),
        };
        let mut procs = procs::sample(&filter, &mut self.prev_procs, &mut self.users);

        // GPUs, and their memory attributed back to the process rows.
        let gpus = self.gpu.sample(self.scope.gpu_indices.as_deref());
        attach_gpu_mem(&mut procs, &gpus);

        if self.history.len() == HISTORY_LEN {
            self.history.pop_front();
        }
        self.history.push_back(pct.round().clamp(0.0, 100.0) as u8);

        Snapshot {
            host: self.host.clone(),
            ts: crate::now_epoch(),
            scope: self.scope.clone(),
            cpu: Cpu {
                pct,
                ncpu: self.ncpu,
                load1,
                load5,
                load15,
            },
            mem,
            gpus,
            procs,
            cpu_history: self.history.iter().copied().collect(),
        }
    }
}

/// Sum each pid's GPU memory across devices onto its process row.
pub fn attach_gpu_mem(procs: &mut [Proc], gpus: &[Gpu]) {
    let mut by_pid: HashMap<u32, u64> = HashMap::new();
    for g in gpus {
        for gp in &g.procs {
            *by_pid.entry(gp.pid).or_default() += gp.mem_mb;
        }
    }
    for p in procs.iter_mut() {
        p.gpu_mem_mb = by_pid.get(&p.pid).copied();
    }
}

impl Snapshot {
    /// Seconds since this snapshot was taken (0 if the clock went backwards).
    pub fn age_secs(&self, now: i64) -> u64 {
        (now - self.ts).max(0) as u64
    }

    /// Older than [`STALE_AFTER_SECS`].
    pub fn is_stale(&self, now: i64) -> bool {
        self.age_secs(now) as i64 > STALE_AFTER_SECS
    }

    /// SM% of `pid` on any GPU, when reported.
    pub fn gpu_sm_pct(&self, pid: u32) -> Option<u8> {
        self.gpus
            .iter()
            .flat_map(|g| g.procs.iter())
            .filter(|p| p.pid == pid)
            .filter_map(|p| p.sm_pct)
            .max()
    }

    /// The status-plugin view: top 8 processes by CPU, one line per GPU,
    /// allocation sizes from the scope (host totals when unscoped).
    pub fn to_host_panel(
        &self,
        job_id: u64,
        job_name: Option<String>,
        age_secs: u64,
    ) -> sint_proto::HostPanel {
        sint_proto::HostPanel {
            host: self.host.clone(),
            job_id,
            job_name,
            age_secs,
            cpu_pct: self.cpu.pct.round().clamp(0.0, 100.0) as u8,
            cpu_alloc: self.scope.cpus_alloc.unwrap_or(self.cpu.ncpu),
            mem_used_mb: self.mem.used_mb,
            mem_alloc_mb: self.scope.mem_alloc_mb.unwrap_or(self.mem.total_mb),
            load1: self.cpu.load1,
            gpus: self
                .gpus
                .iter()
                .map(|g| sint_proto::GpuLine {
                    index: g.index,
                    name: g.name.clone(),
                    util_pct: g.util_pct,
                    mem_used_mb: g.mem_used_mb,
                    mem_total_mb: g.mem_total_mb,
                    temp_c: g.temp_c,
                    power_w: g.power_w,
                })
                .collect(),
            procs: self
                .procs
                .iter()
                .take(8)
                .map(|p| sint_proto::ProcLine {
                    pid: p.pid,
                    user: p.user.clone(),
                    cpu_pct: p.cpu_pct,
                    rss_mb: p.rss_mb,
                    gpu_mem_mb: p.gpu_mem_mb,
                    command: p.command.clone(),
                })
                .collect(),
            cpu_history: self.cpu_history.clone(),
        }
    }
}

/// Read `<jobid>.metrics.json`; `None` when absent or unparseable.
pub fn read_snapshot(dir: &StateDir, job_id: u64) -> Option<Snapshot> {
    let text = std::fs::read_to_string(dir.metrics_file(job_id)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write `<jobid>.metrics.json` atomically, one line plus a newline.
pub fn write_snapshot(dir: &StateDir, job_id: u64, snap: &Snapshot) -> std::io::Result<()> {
    let mut body = serde_json::to_string(snap).map_err(std::io::Error::other)?;
    body.push('\n');
    atomic_write(&dir.metrics_file(job_id), body.as_bytes())
}

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}

/// Short hostname (`c3cpu-e2-u14`, not the FQDN).
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: buf is a valid, writable buffer of the given length.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return "localhost".to_string();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let full = String::from_utf8_lossy(&buf[..end]).to_string();
    full.split('.').next().unwrap_or(&full).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_scope_sampler_sees_this_process() {
        let mut s = Sampler::new(Scope::default());
        assert!(!s.scoped());
        let a = s.sample();
        assert!(!a.host.is_empty());
        assert!(a.cpu.ncpu > 0);
        assert_eq!(a.cpu.pct, 0.0, "first sample has nothing to diff");
        assert!(a.mem.total_mb > 0);
        assert!(a.procs.iter().any(|p| p.pid == std::process::id()));
        assert_eq!(a.cpu_history.len(), 1);
        assert!(a.ts > 0);
        if !s.gpu.available() {
            assert!(a.gpus.is_empty(), "no driver → no GPUs");
            assert!(s.gpu_error().is_some());
        }
        let b = s.sample();
        assert_eq!(b.cpu_history.len(), 2);
        assert!(b.cpu.pct >= 0.0 && b.cpu.pct <= 100.0);
    }

    #[test]
    fn history_is_a_ring() {
        let mut s = Sampler::new(Scope::default());
        for _ in 0..(HISTORY_LEN + 5) {
            s.sample();
        }
        assert_eq!(s.history.len(), HISTORY_LEN);
    }

    #[test]
    fn scoped_sampler_uses_a_fake_v1_cgroup() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job = format!("slurm/uid_{}/job_9", uid());
        let memory = root.join("memory").join(&job);
        let cpuset = root.join("cpuset").join(&job);
        let cpuacct = root.join("cpu,cpuacct").join(&job);
        let w = |p: &Path, s: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, s).unwrap();
        };
        w(&cpuset.join("cpuset.cpus"), "0-1\n");
        w(&memory.join("memory.limit_in_bytes"), "2147483648\n");
        w(&memory.join("memory.usage_in_bytes"), "1073741824\n");
        w(
            &memory.join("step_batch/task_0/cgroup.procs"),
            &format!("{}\n", std::process::id()),
        );
        w(&cpuacct.join("cpuacct.usage"), "1000000000\n");

        let scope = Scope {
            job_id: Some(9),
            cpus_alloc: Some(2),
            ..Default::default()
        };
        let mut s = Sampler::with_cgroup_root(scope, root);
        assert!(s.scoped());
        let a = s.sample();
        assert_eq!(a.mem.total_mb, 2048);
        assert_eq!(a.mem.used_mb, 1024);
        assert_eq!(a.procs.len(), 1);
        assert_eq!(a.procs[0].pid, std::process::id());

        // 0.5 CPU-seconds more, over ~0 s wall → clamps at 100.
        w(&cpuacct.join("cpuacct.usage"), "1500000000\n");
        let b = s.sample();
        assert!(b.cpu.pct > 0.0);
    }

    #[test]
    fn to_host_panel_maps_top_procs_and_gpus() {
        let mut procs: Vec<Proc> = (0..10)
            .map(|i| Proc {
                pid: 100 + i,
                user: "jay".into(),
                cpu_pct: 10.0 * (10 - i) as f32,
                rss_mb: 50,
                threads: 1,
                state: 'R',
                command: format!("cmd{i}"),
                gpu_mem_mb: None,
            })
            .collect();
        let gpus = vec![Gpu {
            index: 1,
            name: "A100".into(),
            util_pct: 87,
            mem_used_mb: 31_000,
            mem_total_mb: 40_000,
            temp_c: Some(65),
            power_w: Some(250),
            power_limit_w: Some(400),
            sm_clock_mhz: Some(1410),
            procs: vec![
                GpuProc {
                    pid: 100,
                    mem_mb: 30_000,
                    sm_pct: Some(80),
                },
                GpuProc {
                    pid: 999,
                    mem_mb: 1_000,
                    sm_pct: None,
                },
            ],
        }];
        attach_gpu_mem(&mut procs, &gpus);
        assert_eq!(procs[0].gpu_mem_mb, Some(30_000));
        assert_eq!(procs[1].gpu_mem_mb, None);

        let snap = Snapshot {
            host: "n1".into(),
            ts: 1_000,
            scope: Scope {
                job_id: Some(7),
                cpus_alloc: Some(4),
                mem_alloc_mb: Some(16_384),
                ..Default::default()
            },
            cpu: Cpu {
                pct: 42.6,
                ncpu: 64,
                load1: 3.5,
                ..Default::default()
            },
            mem: Mem {
                total_mb: 16_384,
                used_mb: 4_096,
            },
            gpus,
            procs,
            cpu_history: vec![1, 2, 3],
        };
        assert_eq!(snap.gpu_sm_pct(100), Some(80));
        assert_eq!(snap.gpu_sm_pct(999), None);
        assert_eq!(snap.age_secs(1_010), 10);
        assert!(!snap.is_stale(1_030));
        assert!(snap.is_stale(1_031));
        assert_eq!(snap.age_secs(900), 0);

        let panel = snap.to_host_panel(7, Some("web".into()), 5);
        assert_eq!(panel.host, "n1");
        assert_eq!(panel.job_id, 7);
        assert_eq!(panel.job_name.as_deref(), Some("web"));
        assert_eq!(panel.age_secs, 5);
        assert_eq!(panel.cpu_pct, 43);
        assert_eq!(panel.cpu_alloc, 4);
        assert_eq!(panel.mem_alloc_mb, 16_384);
        assert_eq!(panel.mem_used_mb, 4_096);
        assert_eq!(panel.load1, 3.5);
        assert_eq!(panel.procs.len(), 8);
        assert_eq!(panel.procs[0].pid, 100);
        assert_eq!(panel.procs[0].gpu_mem_mb, Some(30_000));
        assert_eq!(panel.procs[0].command, "cmd0");
        assert_eq!(panel.gpus.len(), 1);
        assert_eq!(panel.gpus[0].index, 1);
        assert_eq!(panel.gpus[0].util_pct, 87);
        assert_eq!(panel.gpus[0].temp_c, Some(65));
        assert_eq!(panel.cpu_history, vec![1, 2, 3]);

        // Unscoped: host totals stand in for the allocation.
        let mut unscoped = snap.clone();
        unscoped.scope = Scope::default();
        let panel = unscoped.to_host_panel(0, None, 0);
        assert_eq!(panel.cpu_alloc, 64);
        assert_eq!(panel.mem_alloc_mb, 16_384);
    }

    #[test]
    fn snapshot_round_trips_through_the_state_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sd = StateDir(dir.path().join("cache"));
        assert_eq!(read_snapshot(&sd, 5), None);
        let snap = Snapshot {
            host: "n1".into(),
            ts: 42,
            procs: vec![Proc {
                pid: 1,
                state: 'S',
                ..Default::default()
            }],
            ..Default::default()
        };
        write_snapshot(&sd, 5, &snap).unwrap();
        let text = std::fs::read_to_string(sd.metrics_file(5)).unwrap();
        assert!(text.ends_with('\n'));
        assert!(text.contains("\"state\":\"S\""));
        assert_eq!(read_snapshot(&sd, 5), Some(snap));
        assert!(!dir.path().join("cache/5.metrics.json.tmp").exists());

        // Missing keys default, so an older sampler's file still parses.
        let v: Snapshot = serde_json::from_str("{\"host\":\"x\"}").unwrap();
        assert_eq!(v.host, "x");
        assert_eq!(v.scope, Scope::default());
    }
}
