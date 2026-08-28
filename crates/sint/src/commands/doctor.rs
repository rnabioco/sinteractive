//! `sinteractive doctor [--nodes] [--json]` — is this install able to run
//! a session from here, and (with `--nodes`) from every compute node?
//!
//! Local checks are rows of `name / ok|warn|fail / detail`; `fail` means a
//! session cannot work until it is fixed (exit 1), `warn` is something to
//! know about (a GPU driver missing on a login node is normal). The node
//! sweep reports reachability, the binary's version and whether the
//! extracted bundle is visible from each node; it never affects the exit
//! code, since a node being down is Slurm's business, not ours.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use anyhow::Result;
use serde::Serialize;
use sint_core::color::Palette;

use super::common::{current_exe, print_json, ssh_batch, Ctx};
use crate::bundle;
use crate::cli::DoctorArgs;
use crate::zellij_cmd::shell_quote;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
}

impl Check {
    fn new(name: &str, status: Status, detail: impl Into<String>) -> Self {
        Check {
            name: name.to_string(),
            status,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeCheck {
    pub node: String,
    pub reachable: bool,
    pub version: Option<String>,
    pub bundle: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Debug, Serialize)]
struct Report {
    checks: Vec<Check>,
    nodes: Vec<NodeCheck>,
}

/// The Slurm client tools a session launch and its watchers call.
const SLURM_TOOLS: [&str; 5] = ["squeue", "sbatch", "scontrol", "sacct", "sinfo"];

/// Nodes probed at once in the `--nodes` sweep.
const SWEEP_PARALLELISM: usize = 8;

pub fn run(args: DoctorArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let checks = local_checks(&ctx);
    let mut nodes = Vec::new();
    let mut checks = checks;
    if args.nodes {
        match ctx.slurm.node_names() {
            Ok(names) => nodes = sweep_nodes(&ctx, &names),
            Err(e) => checks.push(Check::new(
                "nodes",
                Status::Fail,
                format!("could not list nodes: {e}"),
            )),
        }
    }
    let failed = checks.iter().any(|c| c.status == Status::Fail);

    if args.json {
        print_json(&Report { checks, nodes })?;
    } else {
        let p = ctx.palette(1);
        render(&checks, &nodes, args.nodes, &p);
    }
    Ok(if failed { 1 } else { 0 })
}

fn local_checks(ctx: &Ctx) -> Vec<Check> {
    let mut checks = Vec::new();

    // binary
    let exe = current_exe().unwrap_or_else(|_| PathBuf::from("sinteractive"));
    checks.push(Check::new(
        "binary",
        Status::Ok,
        format!(
            "{} {} (zellij {})",
            exe.display(),
            env!("CARGO_PKG_VERSION"),
            bundle::ZELLIJ_VERSION
        ),
    ));

    // plugin
    checks.push(if bundle::has_plugin() {
        Check::new("plugin", Status::Ok, "status plugin embedded")
    } else {
        Check::new(
            "plugin",
            Status::Fail,
            "built without the status plugin (SINT_SKIP_BUNDLE); sessions cannot start",
        )
    });

    // bundle
    checks.push(match bundle::ensure(&ctx.cfg, true) {
        Ok(b) => Check::new(
            "bundle",
            Status::Ok,
            format!("extracted to {}", b.dir.display()),
        ),
        Err(e) => Check::new(
            "bundle",
            Status::Fail,
            format!("cannot extract into {}: {e:#}", ctx.cfg.cache_dir.display()),
        ),
    });

    // cache
    let cache = &ctx.cfg.cache_dir;
    checks.push(match writable(cache) {
        Ok(()) => {
            let fs = fs_info(cache)
                .map(|(name, total)| format!(" on {name} ({})", fmt_bytes(total)))
                .unwrap_or_default();
            Check::new(
                "cache",
                Status::Ok,
                format!("{} writable{fs}", cache.display()),
            )
        }
        Err(e) => Check::new(
            "cache",
            Status::Fail,
            format!("{} not writable: {e}", cache.display()),
        ),
    });

    // slurm
    let missing: Vec<&str> = SLURM_TOOLS
        .iter()
        .copied()
        .filter(|t| which(t).is_none())
        .collect();
    checks.push(if missing.is_empty() {
        Check::new("slurm", Status::Ok, SLURM_TOOLS.join(" "))
    } else {
        Check::new(
            "slurm",
            Status::Fail,
            format!("not on PATH: {}", missing.join(" ")),
        )
    });

    // cluster
    checks.push(match ctx.slurm.cluster_name() {
        Some(name) => Check::new("cluster", Status::Ok, name),
        None => Check::new(
            "cluster",
            Status::Warn,
            "unknown (scontrol show config did not answer)",
        ),
    });

    // ssh
    checks.push(match which("ssh") {
        Some(p) => Check::new("ssh", Status::Ok, p.display().to_string()),
        None => Check::new(
            "ssh",
            Status::Fail,
            "no ssh client on PATH; peek, send and the readiness wait need one",
        ),
    });

    // shell
    checks.push(match std::env::var("SHELL") {
        Ok(s) if !s.is_empty() => Check::new("shell", Status::Ok, s),
        _ => Check::new("shell", Status::Warn, "SHELL unset; zellij will use sh"),
    });

    // nvml
    checks.push(if nvml_loadable() {
        Check::new("nvml", Status::Ok, "libnvidia-ml.so.1 loadable")
    } else {
        Check::new(
            "nvml",
            Status::Warn,
            "not present (no GPU driver here); GPU monitoring works only where it is",
        )
    });

    // home
    checks.push(home_check());
    checks
}

/// Can the cache dir be created and written to?
fn writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let f = tempfile::Builder::new()
        .prefix(".doctor-")
        .tempfile_in(dir)?;
    std::fs::write(f.path(), b"ok")?;
    Ok(())
}

/// First executable `name` on `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    which_in(name, &std::env::var_os("PATH")?)
}

fn which_in(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path).map(|d| d.join(name)).find(|p| {
        p.metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// `dlopen` the NVIDIA management library, the way the GPU monitor will.
fn nvml_loadable() -> bool {
    // SAFETY: dlopen/dlclose with a valid NUL-terminated name and no other
    // use of the handle.
    unsafe {
        let h = libc::dlopen(
            c"libnvidia-ml.so.1".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_LOCAL,
        );
        if h.is_null() {
            return false;
        }
        libc::dlclose(h);
        true
    }
}

/// Filesystem type name and total size of the filesystem holding `path`.
#[allow(clippy::unnecessary_cast)]
fn fs_info(path: &Path) -> Option<(String, u64)> {
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statfs writes into a zeroed struct of the right type; the
    // path is NUL-terminated.
    let st = unsafe {
        let mut st: libc::statfs = std::mem::zeroed();
        if libc::statfs(c.as_ptr(), &mut st) != 0 {
            return None;
        }
        st
    };
    let total = (st.f_blocks as u64).saturating_mul(st.f_bsize as u64);
    Some((fs_type_name(st.f_type as i64), total))
}

/// Linux `f_type` magic → name (the ones seen on clusters; else the hex).
fn fs_type_name(magic: i64) -> String {
    match magic as u32 {
        0xEF53 => "ext4",
        0x5846_5342 => "xfs",
        0x9123_683E => "btrfs",
        0x0102_1994 => "tmpfs",
        0x794C_7630 => "overlay",
        0x6969 => "nfs",
        0x0BD0_0BD0 => "lustre",
        0x1983_0326 => "beegfs",
        0x4750_4653 => "gpfs",
        0xFF53_4D42 => "cifs",
        0x6573_5546 => "fuse",
        0x0185_8458 | 0x8584_58F6 => "ramfs",
        other => return format!("fs 0x{other:x}"),
    }
    .to_string()
}

/// A local (node-private) filesystem: a cache there is invisible from the
/// compute nodes.
fn is_local_fs(name: &str) -> bool {
    matches!(name, "ext4" | "xfs" | "btrfs" | "tmpfs" | "overlay")
}

/// `$HOME` shared? The cache defaults to `~/.cache/sinteractive`, so a
/// home that is local or tiny (Alpine's 2 GB) needs `SINTERACTIVE_CACHE`
/// pointed somewhere the nodes can see and that has room.
fn home_check() -> Check {
    if std::env::var_os("SINTERACTIVE_CACHE").is_some_and(|v| !v.is_empty()) {
        return Check::new("home", Status::Ok, "cache dir set by SINTERACTIVE_CACHE");
    }
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return Check::new("home", Status::Warn, "HOME unset");
    };
    let home = PathBuf::from(home);
    let Some((name, total)) = fs_info(&home) else {
        return Check::new(
            "home",
            Status::Warn,
            format!("cannot stat {}", home.display()),
        );
    };
    const SMALL: u64 = 5 * 1024 * 1024 * 1024;
    if total < SMALL {
        Check::new(
            "home",
            Status::Warn,
            format!(
                "{} is a small filesystem ({name}, {}); set SINTERACTIVE_CACHE to a roomier shared one",
                home.display(),
                fmt_bytes(total)
            ),
        )
    } else if is_local_fs(&name) {
        Check::new(
            "home",
            Status::Warn,
            format!(
                "{} is on a local filesystem ({name}); compute nodes cannot see it — set SINTERACTIVE_CACHE to a shared one",
                home.display()
            ),
        )
    } else {
        Check::new(
            "home",
            Status::Ok,
            format!("{} on {name} ({})", home.display(), fmt_bytes(total)),
        )
    }
}

fn fmt_bytes(b: u64) -> String {
    const G: f64 = 1024.0 * 1024.0 * 1024.0;
    let g = b as f64 / G;
    if g >= 1024.0 {
        format!("{:.1} TB", g / 1024.0)
    } else if g >= 10.0 {
        format!("{g:.0} GB")
    } else {
        format!("{g:.1} GB")
    }
}

/// The remote probe: this binary's version, then whether the bundle dir
/// exists from there. Chained with `;` so a missing binary still reports
/// the bundle.
fn node_probe(exe: &Path, cache: &Path) -> String {
    format!(
        "{} --version 2>/dev/null; test -d {} && echo sint-bundle=yes || echo sint-bundle=no",
        shell_quote(&exe.to_string_lossy()),
        shell_quote(&cache.join("bin").to_string_lossy())
    )
}

/// Parse the probe's stdout into (version, bundle visible).
fn parse_probe(stdout: &str) -> (Option<String>, bool) {
    let mut version = None;
    let mut bundle = false;
    for line in stdout.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("sinteractive ") {
            version = Some(v.trim().to_string());
        } else if line == "sint-bundle=yes" {
            bundle = true;
        }
    }
    (version, bundle)
}

fn probe_node(node: &str, probe: &str) -> NodeCheck {
    let out = ssh_batch(node, 5, probe).stderr(Stdio::piped()).output();
    let mut check = NodeCheck {
        node: node.to_string(),
        reachable: false,
        version: None,
        bundle: false,
        detail: String::new(),
    };
    match out {
        // 255 is ssh's own failure code; anything else came from the node.
        Ok(out) if out.status.code() != Some(255) => {
            check.reachable = true;
            let (version, bundle) = parse_probe(&String::from_utf8_lossy(&out.stdout));
            check.version = version;
            check.bundle = bundle;
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            check.detail = stderr
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("unreachable")
                .to_string();
        }
        Err(e) => check.detail = format!("ssh: {e}"),
    }
    check
}

/// Probe every node, `SWEEP_PARALLELISM` at a time.
fn sweep_nodes(ctx: &Ctx, names: &[String]) -> Vec<NodeCheck> {
    let exe = current_exe().unwrap_or_else(|_| PathBuf::from("sinteractive"));
    let probe = node_probe(&exe, &ctx.cfg.cache_dir);
    let queue = Mutex::new(names.iter().cloned());
    let results = Mutex::new(Vec::with_capacity(names.len()));
    std::thread::scope(|s| {
        for _ in 0..SWEEP_PARALLELISM.min(names.len().max(1)) {
            s.spawn(|| loop {
                let next = queue.lock().unwrap().next();
                let Some(node) = next else { break };
                let check = probe_node(&node, &probe);
                results.lock().unwrap().push(check);
            });
        }
    });
    let mut checks = results.into_inner().unwrap();
    checks.sort_by(|a, b| a.node.cmp(&b.node));
    checks
}

fn render(checks: &[Check], nodes: &[NodeCheck], swept: bool, p: &Palette) {
    let (reset, bold, dim) = (&p.reset, &p.bold, &p.dim);
    println!("{bold}sinteractive doctor{reset}");
    let width = checks.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in checks {
        let (mark, colour) = match c.status {
            Status::Ok => ("✓", &p.ok),
            Status::Warn => ("!", &p.warn),
            Status::Fail => ("✗", &p.err),
        };
        println!(
            "  {colour}{mark}{reset} {bold}{:<width$}{reset}  {}",
            c.name, c.detail
        );
    }
    if !swept {
        return;
    }
    let reachable = nodes.iter().filter(|n| n.reachable).count();
    let same = nodes
        .iter()
        .filter(|n| n.version.as_deref() == Some(env!("CARGO_PKG_VERSION")))
        .count();
    let bundled = nodes.iter().filter(|n| n.bundle).count();
    println!();
    println!(
        "{bold}Nodes{reset} ({}): {reachable} reachable, {same} with this version, {bundled} see the bundle",
        nodes.len()
    );
    let width = nodes.iter().map(|n| n.node.len()).max().unwrap_or(0);
    for n in nodes {
        if !n.reachable {
            println!(
                "  {}✗{reset} {:<width$}  {dim}{}{reset}",
                p.err, n.node, n.detail
            );
            continue;
        }
        let ok = n.version.as_deref() == Some(env!("CARGO_PKG_VERSION")) && n.bundle;
        let (mark, colour) = if ok { ("✓", &p.ok) } else { ("!", &p.warn) };
        println!(
            "  {colour}{mark}{reset} {:<width$}  {:<12}  {}",
            n.node,
            n.version.as_deref().unwrap_or("binary missing"),
            if n.bundle {
                "bundle visible"
            } else {
                "bundle not visible"
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_parsing() {
        assert_eq!(
            parse_probe("sinteractive 1.0.0-dev\nsint-bundle=yes\n"),
            (Some("1.0.0-dev".to_string()), true)
        );
        assert_eq!(parse_probe("sint-bundle=no\n"), (None, false));
        assert_eq!(parse_probe(""), (None, false));
    }

    #[test]
    fn probe_quotes_paths() {
        let probe = node_probe(Path::new("/opt/s bin/sinteractive"), Path::new("/c"));
        assert!(
            probe.starts_with("'/opt/s bin/sinteractive' --version"),
            "{probe}"
        );
        assert!(
            probe.contains("test -d /c/bin && echo sint-bundle=yes"),
            "{probe}"
        );
    }

    #[test]
    fn fs_names_and_sizes() {
        assert_eq!(fs_type_name(0xEF53), "ext4");
        assert_eq!(fs_type_name(0x4750_4653), "gpfs");
        assert_eq!(fs_type_name(0x1234), "fs 0x1234");
        assert!(is_local_fs("xfs") && !is_local_fs("gpfs"));
        assert_eq!(fmt_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
        assert_eq!(fmt_bytes(250 * 1024 * 1024 * 1024), "250 GB");
        assert_eq!(fmt_bytes(3 * 1024 * 1024 * 1024 * 1024), "3.0 TB");
    }

    #[test]
    fn which_finds_executables() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.path().join("plain"), "").unwrap();
        let path = dir.path().as_os_str();
        assert_eq!(which_in("tool", path), Some(tool));
        assert_eq!(which_in("plain", path), None);
        assert_eq!(which_in("absent", path), None);
    }

    #[test]
    fn statfs_works_on_a_real_dir() {
        let (name, total) = fs_info(Path::new("/")).expect("statfs /");
        assert!(!name.is_empty());
        assert!(total > 0);
    }
}
