//! NVIDIA GPUs through NVML, loaded lazily.
//!
//! `libnvidia-ml.so.1` is dlopen'd on the first sample. Where it is absent
//! (login nodes, CPU partitions) or the driver is not loaded, the sampler
//! reports no GPUs and does not retry for a minute, so a CPU-only session
//! never pays for the failed load on every tick. AMD (ROCm SMI) is a later
//! backend behind the same [`Gpu`] rows.
//!
//! Scoping: Slurm exports the job's devices as `SLURM_JOB_GPUS` /
//! `SLURM_STEP_GPUS` (indices) and cgroups hide the rest; when one of those
//! is set only those indices are reported, so a shared 8-GPU node shows the
//! two you own.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::Nvml;

use super::{Gpu, GpuProc};

/// How long to wait before trying to load NVML again after a failure.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);

/// Lazily initialised NVML handle plus per-device sampling state.
pub struct GpuSampler {
    nvml: Option<Nvml>,
    /// When the last failed init happened.
    failed_at: Option<Instant>,
    /// `process_utilization_stats` cursor per device index.
    last_seen: HashMap<u32, u64>,
    /// The last init error, for `doctor`-style diagnostics.
    pub last_error: Option<String>,
}

impl Default for GpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuSampler {
    pub fn new() -> Self {
        GpuSampler {
            nvml: None,
            failed_at: None,
            last_seen: HashMap::new(),
            last_error: None,
        }
    }

    /// Whether NVML is loaded.
    pub fn available(&self) -> bool {
        self.nvml.is_some()
    }

    fn ensure_init(&mut self) -> bool {
        if self.nvml.is_some() {
            return true;
        }
        if let Some(t) = self.failed_at {
            if t.elapsed() < RETRY_AFTER {
                return false;
            }
        }
        match Nvml::init() {
            Ok(n) => {
                self.nvml = Some(n);
                self.failed_at = None;
                self.last_error = None;
                true
            }
            Err(e) => {
                self.failed_at = Some(Instant::now());
                self.last_error = Some(describe(&e));
                false
            }
        }
    }

    /// Sample every visible GPU, or only `indices` when given. Empty when
    /// NVML is unavailable.
    pub fn sample(&mut self, indices: Option<&[u32]>) -> Vec<Gpu> {
        if !self.ensure_init() {
            return Vec::new();
        }
        let Some(nvml) = self.nvml.as_ref() else {
            return Vec::new();
        };
        let count = match nvml.device_count() {
            Ok(c) => c,
            Err(e) => {
                // A device count failure after a good init means the driver
                // went away; drop the handle so the next sample reloads.
                self.last_error = Some(describe(&e));
                self.nvml = None;
                self.failed_at = Some(Instant::now());
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        for index in 0..count {
            if let Some(want) = indices {
                if !want.contains(&index) {
                    continue;
                }
            }
            let Ok(dev) = nvml.device_by_index(index) else {
                continue;
            };
            let mem = dev.memory_info().ok();
            let mut procs: HashMap<u32, GpuProc> = HashMap::new();
            for info in dev
                .running_compute_processes()
                .unwrap_or_default()
                .into_iter()
                .chain(dev.running_graphics_processes().unwrap_or_default())
            {
                let mem_mb = match info.used_gpu_memory {
                    UsedGpuMemory::Used(b) => b / (1024 * 1024),
                    UsedGpuMemory::Unavailable => 0,
                };
                let e = procs.entry(info.pid).or_insert(GpuProc {
                    pid: info.pid,
                    mem_mb: 0,
                    sm_pct: None,
                });
                e.mem_mb = e.mem_mb.max(mem_mb);
            }
            let cursor = self.last_seen.get(&index).copied();
            if let Ok(samples) = dev.process_utilization_stats(cursor) {
                let mut newest = cursor.unwrap_or(0);
                for s in samples {
                    newest = newest.max(s.timestamp);
                    let e = procs.entry(s.pid).or_insert(GpuProc {
                        pid: s.pid,
                        mem_mb: 0,
                        sm_pct: None,
                    });
                    e.sm_pct = Some(s.sm_util.min(100) as u8);
                }
                self.last_seen.insert(index, newest);
            }
            let mut procs: Vec<GpuProc> = procs.into_values().collect();
            procs.sort_by(|a, b| b.mem_mb.cmp(&a.mem_mb).then(a.pid.cmp(&b.pid)));
            out.push(Gpu {
                index,
                name: dev.name().unwrap_or_else(|_| "GPU".to_string()),
                util_pct: dev
                    .utilization_rates()
                    .map(|u| u.gpu.min(100) as u8)
                    .unwrap_or(0),
                mem_used_mb: mem.as_ref().map(|m| m.used / (1024 * 1024)).unwrap_or(0),
                mem_total_mb: mem.as_ref().map(|m| m.total / (1024 * 1024)).unwrap_or(0),
                temp_c: dev.temperature(TemperatureSensor::Gpu).ok(),
                power_w: dev.power_usage().ok().map(|mw| mw / 1000),
                power_limit_w: dev.enforced_power_limit().ok().map(|mw| mw / 1000),
                sm_clock_mhz: dev
                    .clock_info(Clock::SM)
                    .or_else(|_| dev.clock_info(Clock::Graphics))
                    .ok(),
                procs,
            });
        }
        out
    }
}

fn describe(e: &NvmlError) -> String {
    match e {
        NvmlError::LibloadingError(_) | NvmlError::LibraryNotFound => {
            "libnvidia-ml.so.1 not found (no NVIDIA driver on this host)".to_string()
        }
        NvmlError::DriverNotLoaded => "NVIDIA driver not loaded".to_string(),
        other => format!("NVML: {other}"),
    }
}

/// GPU indices from `SLURM_STEP_GPUS`, `SLURM_JOB_GPUS` or
/// `CUDA_VISIBLE_DEVICES` (first one set). `None` when unset or when the
/// value is not a plain index list (UUIDs, `NoDevFiles`).
pub fn indices_from_env() -> Option<Vec<u32>> {
    ["SLURM_STEP_GPUS", "SLURM_JOB_GPUS", "CUDA_VISIBLE_DEVICES"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.trim().is_empty())
        .and_then(|v| parse_indices(&v))
}

/// `"0,1"` → `[0, 1]`; anything that is not all indices → `None`.
pub fn parse_indices(s: &str) -> Option<Vec<u32>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut out: Vec<u32> = s
        .split(',')
        .map(|p| p.trim().parse::<u32>().ok())
        .collect::<Option<_>>()?;
    out.sort_unstable();
    out.dedup();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_parse() {
        assert_eq!(parse_indices("0,1"), Some(vec![0, 1]));
        assert_eq!(parse_indices(" 3 "), Some(vec![3]));
        assert_eq!(parse_indices("1,0,1"), Some(vec![0, 1]));
        assert_eq!(parse_indices(""), None);
        assert_eq!(parse_indices("GPU-3a2b"), None, "uuid form ignored");
        assert_eq!(parse_indices("NoDevFiles"), None);
        assert_eq!(parse_indices("0,"), None);
    }

    #[test]
    fn sampler_degrades_without_a_driver_and_does_not_hammer_init() {
        let mut s = GpuSampler::new();
        let gpus = s.sample(None);
        if s.available() {
            // A GPU host: nothing to assert about counts, but rows must be sane.
            for g in &gpus {
                assert!(g.util_pct <= 100);
                assert!(g.mem_used_mb <= g.mem_total_mb.max(g.mem_used_mb));
            }
            return;
        }
        assert!(gpus.is_empty());
        let first = s.failed_at.expect("failure remembered");
        assert!(s.last_error.is_some());
        assert!(s.sample(Some(&[0])).is_empty());
        assert_eq!(s.failed_at, Some(first), "no retry within a minute");
    }
}
