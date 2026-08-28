//! `SINTERACTIVE_*` environment and well-known paths.
//!
//! Explicit CLI flags always win over these; these win over built-in
//! defaults. Defaults match Bodhi (the origin cluster); Alpine users override
//! `partition`/`qos` in their shell profile as documented in the README.

use std::path::PathBuf;
use std::str::FromStr;

/// Resolved configuration. Build with [`Config::from_env`].
#[derive(Debug, Clone)]
pub struct Config {
    /// `SINTERACTIVE_TIME` — default walltime in Slurm form (`24:00:00`).
    /// Stored as given; the CLI normalises human forms (`8h`) through
    /// [`crate::time::parse_time`] at the point of use, as the script did.
    pub time: String,
    /// `SINTERACTIVE_PARTITION` — default partition (`interactive`).
    pub partition: String,
    /// `SINTERACTIVE_QOS` — added as `--qos` only when set.
    pub qos: Option<String>,
    /// `SINTERACTIVE_CPUS` — default `--cpus-per-task` (2).
    pub cpus: u32,
    /// `SINTERACTIVE_MEM` — default `--mem` (`8G`).
    pub mem: String,
    /// `SINTERACTIVE_MOUSE` — `on/1/true/yes` enables mouse mode,
    /// `off/0/false/no` disables it. Default **on** (0.x defaulted to off).
    pub mouse: bool,
    /// `SINTERACTIVE_CACHE` — state dir; default `$XDG_CACHE_HOME/sinteractive`
    /// or `~/.cache/sinteractive`.
    pub cache_dir: PathBuf,
    /// `SINTERACTIVE_SHARE` — asset root override for `install-claude`.
    pub share_dir: Option<PathBuf>,
    /// `SINTERACTIVE_WARN_YELLOW` (3600) / `SINTERACTIVE_WARN_RED` (600) /
    /// `SINTERACTIVE_GRACE` (10) / `SINTERACTIVE_POLL` (30, floor 5).
    pub warn_yellow: i64,
    pub warn_red: i64,
    pub grace: i64,
    pub poll: i64,
    /// `SINTERACTIVE_QUOTA_POLL` (600, floor 30), `_FILE`, `_HOSTS`, `_PORT`, `_TIMEOUT` (5).
    pub quota_poll: i64,
    pub quota_file: PathBuf,
    pub quota_hosts: Vec<String>,
    pub quota_port: u16,
    pub quota_timeout: u64,
    /// `SINTERACTIVE_AGENT_WARN` (1800) — walltime-guard hook threshold.
    pub agent_warn: i64,
    /// `SINTERACTIVE_JOB_ID` / `SINTERACTIVE_NAME` — set inside a session.
    pub job_id: Option<u64>,
    pub name: Option<String>,
    /// `SINTERACTIVE_THEME` — `dark`/`light` force a theme; `auto`/unset
    /// (`None`) lets [`crate::theme::Theme::detect`] ask the terminal.
    pub theme: Option<crate::theme::Mode>,
}

/// Default quota file (Bodhi).
pub const QUOTA_FILE_DEFAULT: &str = "/cluster/scripts/quota_current.txt";
/// Default quota daemon port (Bodhi).
pub const QUOTA_PORT_DEFAULT: u16 = 9878;

/// Default quota daemon hosts (Bodhi): `172.20.8.110` through `.118`.
pub fn default_quota_hosts() -> Vec<String> {
    (110..=118).map(|n| format!("172.20.8.{n}")).collect()
}

/// A `SINTERACTIVE_*` value, treating unset and empty alike (the script's
/// `${VAR:-default}` semantics).
fn env_str(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Parse an env var, falling back to `default` when unset or unparseable.
fn env_parse<T: FromStr>(key: &str, default: T) -> T {
    env_str(key)
        .and_then(|v| v.trim().parse::<T>().ok())
        .unwrap_or(default)
}

/// `on/1/true/yes` → true, `off/0/false/no` → false, anything else → `None`.
pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Some(true),
        "off" | "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// Split a host list on commas and whitespace, dropping empties.
pub fn parse_hosts(value: &str) -> Vec<String> {
    value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|h| !h.is_empty())
        .map(str::to_string)
        .collect()
}

/// `$SINTERACTIVE_CACHE`, else `$XDG_CACHE_HOME/sinteractive`, else
/// `~/.cache/sinteractive`.
fn cache_dir_from_env() -> PathBuf {
    if let Some(dir) = env_str("SINTERACTIVE_CACHE") {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = env_str("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("sinteractive");
    }
    let home = env_str("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".cache").join("sinteractive")
}

impl Config {
    /// Built-in defaults with no `SINTERACTIVE_*` variable consulted.
    /// `cache_dir` still derives from `$HOME`/`$XDG_CACHE_HOME` because there
    /// is no other sensible value for it.
    pub fn defaults() -> Self {
        Config {
            time: "24:00:00".to_string(),
            partition: "interactive".to_string(),
            qos: None,
            cpus: 2,
            mem: "8G".to_string(),
            mouse: true,
            cache_dir: cache_dir_from_env(),
            share_dir: None,
            warn_yellow: 3600,
            warn_red: 600,
            grace: 10,
            poll: 30,
            quota_poll: 600,
            quota_file: PathBuf::from(QUOTA_FILE_DEFAULT),
            quota_hosts: default_quota_hosts(),
            quota_port: QUOTA_PORT_DEFAULT,
            quota_timeout: 5,
            agent_warn: 1800,
            job_id: None,
            name: None,
            theme: None,
        }
    }

    /// Read every `SINTERACTIVE_*` variable, applying defaults and floors.
    /// Never fails: an unparseable value falls back to the default.
    pub fn from_env() -> Self {
        let d = Config::defaults();
        let mut c = Config {
            time: env_str("SINTERACTIVE_TIME").unwrap_or(d.time),
            partition: env_str("SINTERACTIVE_PARTITION").unwrap_or(d.partition),
            qos: env_str("SINTERACTIVE_QOS"),
            cpus: env_parse("SINTERACTIVE_CPUS", d.cpus),
            mem: env_str("SINTERACTIVE_MEM").unwrap_or(d.mem),
            mouse: env_str("SINTERACTIVE_MOUSE")
                .and_then(|v| parse_bool(&v))
                .unwrap_or(d.mouse),
            cache_dir: d.cache_dir,
            share_dir: env_str("SINTERACTIVE_SHARE").map(PathBuf::from),
            warn_yellow: env_parse("SINTERACTIVE_WARN_YELLOW", d.warn_yellow),
            warn_red: env_parse("SINTERACTIVE_WARN_RED", d.warn_red),
            grace: env_parse("SINTERACTIVE_GRACE", d.grace),
            poll: env_parse("SINTERACTIVE_POLL", d.poll),
            quota_poll: env_parse("SINTERACTIVE_QUOTA_POLL", d.quota_poll),
            quota_file: env_str("SINTERACTIVE_QUOTA_FILE")
                .map(PathBuf::from)
                .unwrap_or(d.quota_file),
            quota_hosts: env_str("SINTERACTIVE_QUOTA_HOSTS")
                .map(|v| parse_hosts(&v))
                .filter(|h| !h.is_empty())
                .unwrap_or(d.quota_hosts),
            quota_port: env_parse("SINTERACTIVE_QUOTA_PORT", d.quota_port),
            quota_timeout: env_parse("SINTERACTIVE_QUOTA_TIMEOUT", d.quota_timeout),
            agent_warn: env_parse("SINTERACTIVE_AGENT_WARN", d.agent_warn),
            job_id: env_str("SINTERACTIVE_JOB_ID").and_then(|v| v.trim().parse().ok()),
            name: env_str("SINTERACTIVE_NAME"),
            theme: env_str("SINTERACTIVE_THEME").and_then(|v| crate::theme::Mode::parse(&v)),
        };
        // Floors, as in the script: the loop must not spin on the scheduler.
        if c.poll < 5 {
            c.poll = 5;
        }
        if c.quota_poll < 30 {
            c.quota_poll = 30;
        }
        c
    }
}

/// `SINTERACTIVE_COLOR` — `auto` (default), `always`, `never`; `NO_COLOR` honoured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    /// `never|no|0` → Never, `always|yes|1` → Always, anything else → Auto.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "never" | "no" | "0" => ColorMode::Never,
            "always" | "yes" | "1" => ColorMode::Always,
            _ => ColorMode::Auto,
        }
    }

    pub fn from_env() -> Self {
        env_str("SINTERACTIVE_COLOR")
            .map(|v| ColorMode::parse(&v))
            .unwrap_or(ColorMode::Auto)
    }
}

/// Test-only serialisation of environment mutation: `std::env::set_var` is
/// process-global and the test harness runs tests on several threads.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    /// Hold this for the duration of any test that reads or writes env vars.
    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every variable the crate reads, so tests can start from a clean slate.
    pub(crate) const ALL_VARS: &[&str] = &[
        "SINTERACTIVE_TIME",
        "SINTERACTIVE_PARTITION",
        "SINTERACTIVE_QOS",
        "SINTERACTIVE_CPUS",
        "SINTERACTIVE_MEM",
        "SINTERACTIVE_MOUSE",
        "SINTERACTIVE_CACHE",
        "SINTERACTIVE_SHARE",
        "SINTERACTIVE_WARN_YELLOW",
        "SINTERACTIVE_WARN_RED",
        "SINTERACTIVE_GRACE",
        "SINTERACTIVE_POLL",
        "SINTERACTIVE_QUOTA_POLL",
        "SINTERACTIVE_QUOTA_FILE",
        "SINTERACTIVE_QUOTA_HOSTS",
        "SINTERACTIVE_QUOTA_PORT",
        "SINTERACTIVE_QUOTA_TIMEOUT",
        "SINTERACTIVE_AGENT_WARN",
        "SINTERACTIVE_JOB_ID",
        "SINTERACTIVE_NAME",
        "SINTERACTIVE_THEME",
        "SINTERACTIVE_COLOR",
        "NO_COLOR",
        "COLORFGBG",
        "XDG_CACHE_HOME",
    ];

    /// Snapshot of the variables in [`ALL_VARS`] plus `TERM` and `HOME`,
    /// restored on drop so one test cannot leak into the next.
    pub(crate) struct EnvRestore(Vec<(String, Option<String>)>);

    impl EnvRestore {
        /// Capture, then clear every variable in [`ALL_VARS`].
        pub(crate) fn clean() -> Self {
            let mut saved = Vec::new();
            for k in ALL_VARS.iter().chain(["TERM", "HOME"].iter()) {
                saved.push((k.to_string(), std::env::var(k).ok()));
            }
            for k in ALL_VARS {
                std::env::remove_var(k);
            }
            EnvRestore(saved)
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (k, v) in &self.0 {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_env::{lock, EnvRestore};
    use super::*;

    #[test]
    fn defaults_when_nothing_set() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("HOME", "/home/test");
        let c = Config::from_env();
        assert_eq!(c.time, "24:00:00");
        assert_eq!(c.partition, "interactive");
        assert_eq!(c.qos, None);
        assert_eq!(c.cpus, 2);
        assert_eq!(c.mem, "8G");
        assert!(c.mouse, "mouse defaults on");
        assert_eq!(c.cache_dir, PathBuf::from("/home/test/.cache/sinteractive"));
        assert_eq!(c.share_dir, None);
        assert_eq!(c.warn_yellow, 3600);
        assert_eq!(c.warn_red, 600);
        assert_eq!(c.grace, 10);
        assert_eq!(c.poll, 30);
        assert_eq!(c.quota_poll, 600);
        assert_eq!(c.quota_file, PathBuf::from(QUOTA_FILE_DEFAULT));
        assert_eq!(c.quota_hosts.len(), 9);
        assert_eq!(c.quota_hosts[0], "172.20.8.110");
        assert_eq!(c.quota_hosts[8], "172.20.8.118");
        assert_eq!(c.quota_port, 9878);
        assert_eq!(c.quota_timeout, 5);
        assert_eq!(c.agent_warn, 1800);
        assert_eq!(c.job_id, None);
        assert_eq!(c.name, None);
        assert_eq!(c.theme, None);
    }

    #[test]
    fn overrides_and_floors() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("SINTERACTIVE_TIME", "8h");
        std::env::set_var("SINTERACTIVE_PARTITION", "acpu");
        std::env::set_var("SINTERACTIVE_QOS", "cpu-normal");
        std::env::set_var("SINTERACTIVE_CPUS", "8");
        std::env::set_var("SINTERACTIVE_MEM", "32G");
        std::env::set_var("SINTERACTIVE_MOUSE", "off");
        std::env::set_var("SINTERACTIVE_CACHE", "/tmp/sint-cache");
        std::env::set_var("SINTERACTIVE_SHARE", "/opt/share");
        std::env::set_var("SINTERACTIVE_WARN_YELLOW", "1200");
        std::env::set_var("SINTERACTIVE_WARN_RED", "120");
        std::env::set_var("SINTERACTIVE_GRACE", "55");
        std::env::set_var("SINTERACTIVE_POLL", "1");
        std::env::set_var("SINTERACTIVE_QUOTA_POLL", "5");
        std::env::set_var("SINTERACTIVE_QUOTA_FILE", "/x/quota.txt");
        std::env::set_var("SINTERACTIVE_QUOTA_HOSTS", "10.0.0.1, 10.0.0.2 10.0.0.3");
        std::env::set_var("SINTERACTIVE_QUOTA_PORT", "1234");
        std::env::set_var("SINTERACTIVE_QUOTA_TIMEOUT", "2");
        std::env::set_var("SINTERACTIVE_AGENT_WARN", "900");
        std::env::set_var("SINTERACTIVE_JOB_ID", "147845");
        std::env::set_var("SINTERACTIVE_NAME", "mywork");
        std::env::set_var("SINTERACTIVE_THEME", "light");
        let c = Config::from_env();
        assert_eq!(c.time, "8h");
        assert_eq!(c.partition, "acpu");
        assert_eq!(c.qos.as_deref(), Some("cpu-normal"));
        assert_eq!(c.cpus, 8);
        assert_eq!(c.mem, "32G");
        assert!(!c.mouse);
        assert_eq!(c.cache_dir, PathBuf::from("/tmp/sint-cache"));
        assert_eq!(c.share_dir, Some(PathBuf::from("/opt/share")));
        assert_eq!(c.warn_yellow, 1200);
        assert_eq!(c.warn_red, 120);
        assert_eq!(c.grace, 55);
        assert_eq!(c.poll, 5, "poll floor");
        assert_eq!(c.quota_poll, 30, "quota poll floor");
        assert_eq!(c.quota_file, PathBuf::from("/x/quota.txt"));
        assert_eq!(c.quota_hosts, vec!["10.0.0.1", "10.0.0.2", "10.0.0.3"]);
        assert_eq!(c.quota_port, 1234);
        assert_eq!(c.quota_timeout, 2);
        assert_eq!(c.agent_warn, 900);
        assert_eq!(c.job_id, Some(147845));
        assert_eq!(c.name.as_deref(), Some("mywork"));
        assert_eq!(c.theme, Some(crate::theme::Mode::Light));
    }

    #[test]
    fn unparseable_values_fall_back() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("SINTERACTIVE_CPUS", "lots");
        std::env::set_var("SINTERACTIVE_POLL", "soon");
        std::env::set_var("SINTERACTIVE_QUOTA_PORT", "99999");
        std::env::set_var("SINTERACTIVE_MOUSE", "maybe");
        std::env::set_var("SINTERACTIVE_THEME", "auto");
        std::env::set_var("SINTERACTIVE_JOB_ID", "abc");
        std::env::set_var("SINTERACTIVE_QUOTA_HOSTS", " , ");
        let c = Config::from_env();
        assert_eq!(c.cpus, 2);
        assert_eq!(c.poll, 30);
        assert_eq!(c.quota_port, 9878);
        assert!(c.mouse);
        assert_eq!(c.theme, None);
        assert_eq!(c.job_id, None);
        assert_eq!(c.quota_hosts.len(), 9);
    }

    #[test]
    fn xdg_cache_home_is_honoured() {
        let _g = lock();
        let _r = EnvRestore::clean();
        std::env::set_var("XDG_CACHE_HOME", "/xdg");
        assert_eq!(
            Config::from_env().cache_dir,
            PathBuf::from("/xdg/sinteractive")
        );
        std::env::set_var("SINTERACTIVE_CACHE", "/explicit");
        assert_eq!(Config::from_env().cache_dir, PathBuf::from("/explicit"));
    }

    #[test]
    fn mouse_spellings() {
        for v in ["on", "1", "true", "yes", "YES", "On"] {
            assert_eq!(parse_bool(v), Some(true), "{v}");
        }
        for v in ["off", "0", "false", "no", "NO"] {
            assert_eq!(parse_bool(v), Some(false), "{v}");
        }
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn color_mode_from_env() {
        let _g = lock();
        let _r = EnvRestore::clean();
        assert_eq!(ColorMode::from_env(), ColorMode::Auto);
        for (v, want) in [
            ("never", ColorMode::Never),
            ("no", ColorMode::Never),
            ("0", ColorMode::Never),
            ("always", ColorMode::Always),
            ("yes", ColorMode::Always),
            ("1", ColorMode::Always),
            ("auto", ColorMode::Auto),
            ("bogus", ColorMode::Auto),
        ] {
            std::env::set_var("SINTERACTIVE_COLOR", v);
            assert_eq!(ColorMode::from_env(), want, "{v}");
        }
    }
}
