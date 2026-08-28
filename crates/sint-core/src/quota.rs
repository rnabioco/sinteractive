//! Storage quota (Bodhi).
//!
//! Ports script lines 1252-1476. Bodhi's `quota_check` lives only on the head
//! node, but its daemons answer compute nodes directly and the hard-limit
//! file is on shared storage, so a session can probe from where it runs.
//!
//! - hard limit: awk over `user|size|email` lines in `SINTERACTIVE_QUOTA_FILE`
//!   (read **first** — local, and it gates the network half)
//! - usage: for each host, TCP connect, send `QUOTA <uid>\n`, read `OK <kb>`,
//!   sum. Down daemons are skipped; **zero answers is a hard failure** (a
//!   partial sum could silently clear a real warning). Connect/read timeouts
//!   are real here, unlike bash `/dev/tcp`.
//! - cache: `quota.json`, per user (not per job), so per-job teardown leaves it.
//!
//! On clusters without the daemons (Alpine) every call reports "unavailable"
//! and no notice is ever produced.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{anyhow, Context};
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::Config;
use crate::state::{atomic_write, StateDir};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuotaSnapshot {
    pub user: String,
    pub used_kb: u64,
    pub hard_kb: u64,
    pub over_kb: u64,
    /// Percent used, integer. The 0.x cache wrote `%.1f`; reading truncates.
    #[serde(deserialize_with = "de_pct")]
    pub pct: u64,
    /// The 0.x cache wrote `0`/`1`; reading accepts either form.
    #[serde(deserialize_with = "de_over")]
    pub over: bool,
    pub checked_epoch: i64,
}

/// Accept `12`, `12.3` (0.x wrote `%.1f`), or `"12.3"`.
fn de_pct<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().map(|f| f.max(0.0) as u64))
            .ok_or_else(|| serde::de::Error::custom("pct is not a number")),
        serde_json::Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map(|f| f.max(0.0) as u64)
            .map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("pct is not a number")),
    }
}

/// Accept `true`/`false` or `1`/`0`.
fn de_over<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Bool(b) => Ok(b),
        serde_json::Value::Number(n) => Ok(n.as_i64().unwrap_or(0) != 0),
        _ => Err(serde::de::Error::custom("over is not a bool")),
    }
}

/// `500G` → KiB; IEC ladder (`K`,`M`,`G`,`T`,`P`), bare number = KiB.
/// Accepts `12.5G`, `4000M`, `30.0TiB`, `512MB`; the result is truncated
/// like awk's `%d`.
pub fn size_to_kb(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.')
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let number: f64 = s[..digits].parse().ok()?;
    let mut unit = s[digits..].trim().to_ascii_uppercase();
    if let Some(u) = unit.strip_suffix("IB") {
        unit = u.to_string();
    } else if let Some(u) = unit.strip_suffix('B') {
        unit = u.to_string();
    }
    let mult: f64 = match unit.as_str() {
        "" | "K" => 1.0,
        "M" => 1024.0,
        "G" => 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0,
        "P" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let kb = number * mult;
    if !kb.is_finite() || kb < 0.0 {
        return None;
    }
    Some(kb as u64)
}

/// KiB → human (`12.3G`, `500G`, `1.2T`), mirroring `quota_check`'s output:
/// the largest unit with a value ≥ 1, one decimal unless it is `.0`.
pub fn kb_to_size(kb: u64) -> String {
    const UNITS: [&str; 5] = ["K", "M", "G", "T", "P"];
    let mut v = kb as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let s = format!("{v:.1}");
    let s = s.strip_suffix(".0").unwrap_or(&s);
    format!("{s}{}", UNITS[i])
}

/// Hard limit for `user` from the quota file contents (`user|size|email`
/// lines; first exact match on the user field wins).
pub fn parse_hard_kb(file_contents: &str, user: &str) -> Option<u64> {
    file_contents.lines().find_map(|line| {
        let mut fields = line.split('|');
        let u = fields.next()?.trim();
        if u != user {
            return None;
        }
        let size = fields.next()?.trim();
        if size.is_empty() {
            return None;
        }
        size_to_kb(size)
    })
}

/// Ask one daemon for `uid`'s usage. `None` for any failure — connect, write,
/// read timeout, or an answer that is not `OK <kb>`.
fn ask_daemon(host: &str, port: u16, uid: u32, timeout: Duration) -> Option<u64> {
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    stream.write_all(format!("QUOTA {uid}\n").as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    parse_ok_line(&line)
}

/// `OK 12345` (trailing junk after the digits ignored, as the script did).
fn parse_ok_line(line: &str) -> Option<u64> {
    let rest = line.trim_end_matches(['\r', '\n']).strip_prefix("OK ")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Usage summed over every daemon that answers. `Err` when none did.
pub fn used_kb(cfg: &Config, uid: u32) -> anyhow::Result<u64> {
    let timeout = Duration::from_secs(cfg.quota_timeout.max(1));
    let mut total: u64 = 0;
    let mut answered = 0usize;
    for host in &cfg.quota_hosts {
        if let Some(kb) = ask_daemon(host, cfg.quota_port, uid, timeout) {
            total = total.saturating_add(kb);
            answered += 1;
        }
    }
    if answered == 0 {
        return Err(anyhow!(
            "no quota daemon answered ({} host(s) on port {})",
            cfg.quota_hosts.len(),
            cfg.quota_port
        ));
    }
    Ok(total)
}

/// The calling user's name and uid, as the 0.x `${USER:-$(id -un)}` and
/// `id -u` reported them: `USER`, else `LOGNAME`, else the uid as a string.
pub fn current_user() -> (String, u32) {
    // SAFETY: getuid(2) has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let name = ["USER", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| uid.to_string());
    (name, uid)
}

/// Probe now. `Err` when the file has no entry, or no daemon answered.
pub fn probe(cfg: &Config, user: &str, uid: u32) -> anyhow::Result<QuotaSnapshot> {
    // Hard limit first: a local read that fails instantly on a cluster
    // without the file, and it gates the network half.
    let contents = fs::read_to_string(&cfg.quota_file)
        .with_context(|| format!("quota file {}", cfg.quota_file.display()))?;
    let hard_kb = parse_hard_kb(&contents, user)
        .ok_or_else(|| anyhow!("no quota entry for {user} in {}", cfg.quota_file.display()))?;
    if hard_kb == 0 {
        return Err(anyhow!("quota entry for {user} has a zero limit"));
    }
    let used = used_kb(cfg, uid)?;
    Ok(snapshot(user, used, hard_kb, crate::now_epoch()))
}

/// Derive the snapshot fields from a usage/limit pair.
pub fn snapshot(user: &str, used_kb: u64, hard_kb: u64, checked_epoch: i64) -> QuotaSnapshot {
    let pct = if hard_kb == 0 {
        0
    } else {
        ((used_kb as u128 * 100) / hard_kb as u128) as u64
    };
    QuotaSnapshot {
        user: user.to_string(),
        used_kb,
        hard_kb,
        over_kb: used_kb.saturating_sub(hard_kb),
        pct,
        over: used_kb > hard_kb,
        checked_epoch,
    }
}

impl QuotaSnapshot {
    /// Seconds since the probe, given `now`; never negative.
    pub fn age(&self, now: i64) -> i64 {
        (now - self.checked_epoch).max(0)
    }
}

/// Read the cache.
pub fn cached(dir: &StateDir) -> Option<QuotaSnapshot> {
    let text = fs::read_to_string(dir.quota_file()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write the cache atomically, key order
/// `user,used_kb,hard_kb,over_kb,pct,over,checked_epoch`.
pub fn write_cache(dir: &StateDir, q: &QuotaSnapshot) -> io::Result<()> {
    let mut body = serde_json::to_string(q).map_err(io::Error::other)?;
    body.push('\n');
    atomic_write(&dir.quota_file(), body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn size_to_kb_ladder() {
        assert_eq!(size_to_kb("500G"), Some(524288000));
        assert_eq!(size_to_kb("12.5G"), Some(13107200));
        assert_eq!(size_to_kb("4000M"), Some(4096000));
        assert_eq!(size_to_kb("1.5T"), Some(1610612736));
        assert_eq!(size_to_kb("1024"), Some(1024));
        assert_eq!(size_to_kb("1024K"), Some(1024));
        assert_eq!(size_to_kb("30.0TiB"), Some(32212254720));
        assert_eq!(size_to_kb("512MB"), Some(524288));
        assert_eq!(size_to_kb(" 2 G "), Some(2097152));
        assert_eq!(size_to_kb("1P"), Some(1u64 << 40));
        assert_eq!(size_to_kb("12.3G"), Some(12897484), "awk %d truncates");
        assert_eq!(size_to_kb(""), None);
        assert_eq!(size_to_kb("G"), None);
        assert_eq!(size_to_kb("5X"), None);
        assert_eq!(size_to_kb("abc"), None);
    }

    #[test]
    fn kb_to_size_ladder() {
        assert_eq!(kb_to_size(0), "0K");
        assert_eq!(kb_to_size(512), "512K");
        assert_eq!(kb_to_size(1024), "1M");
        assert_eq!(kb_to_size(1536), "1.5M");
        assert_eq!(kb_to_size(524288000), "500G");
        assert_eq!(kb_to_size(1610612736), "1.5T");
        assert_eq!(kb_to_size(1u64 << 40), "1P");
        assert_eq!(kb_to_size(1u64 << 50), "1024P", "no unit above P");
    }

    #[test]
    fn size_round_trips() {
        for (s, back) in [
            ("500G", "500G"),
            ("12.3G", "12.3G"),
            ("1.5T", "1.5T"),
            ("4000M", "3.9G"),
            ("1M", "1M"),
        ] {
            let kb = size_to_kb(s).expect(s);
            assert_eq!(kb_to_size(kb), back, "{s}");
        }
    }

    #[test]
    fn parse_hard_kb_matches_the_user_exactly() {
        let file = "jayne|1T|jayne@example.org\n\
                    jay | 500G | jay@example.org\n\
                    jay|999G|dup@example.org\n\
                    # comment\n\
                    noquota|\n\
                    bob|nonsense|bob@example.org\n";
        assert_eq!(
            parse_hard_kb(file, "jay"),
            Some(524288000),
            "first match wins"
        );
        assert_eq!(parse_hard_kb(file, "jayne"), Some(1u64 << 30));
        assert_eq!(parse_hard_kb(file, "ja"), None);
        assert_eq!(parse_hard_kb(file, "noquota"), None);
        assert_eq!(parse_hard_kb(file, "bob"), None);
        assert_eq!(parse_hard_kb(file, "nobody"), None);
        assert_eq!(parse_hard_kb("", "jay"), None);
    }

    #[test]
    fn ok_line_parsing() {
        assert_eq!(parse_ok_line("OK 123\n"), Some(123));
        assert_eq!(parse_ok_line("OK 123 extra\r\n"), Some(123));
        assert_eq!(parse_ok_line("OK\n"), None);
        assert_eq!(parse_ok_line("OK x\n"), None);
        assert_eq!(parse_ok_line("ERR 1\n"), None);
        assert_eq!(parse_ok_line(""), None);
    }

    #[test]
    fn snapshot_maths() {
        let s = snapshot("jay", 537185280, 524288000, 100);
        assert!(s.over);
        assert_eq!(s.over_kb, 12897280);
        assert_eq!(s.pct, 102);
        let s = snapshot("jay", 262144000, 524288000, 100);
        assert!(!s.over);
        assert_eq!(s.over_kb, 0);
        assert_eq!(s.pct, 50);
        assert_eq!(s.age(160), 60);
        assert_eq!(s.age(50), 0);
    }

    /// A fake daemon: answers each connection with `reply` once.
    fn spawn_daemon(
        reply: &'static str,
        connections: usize,
    ) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..connections {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                requests.push(line);
                let mut stream = reader.into_inner();
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            }
            requests
        });
        (port, handle)
    }

    /// A port nothing listens on (bound, then released).
    fn closed_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").port()
    }

    fn test_config(dir: &std::path::Path, hosts: Vec<String>, port: u16) -> Config {
        let mut cfg = Config::defaults();
        cfg.quota_file = dir.join("quota_current.txt");
        cfg.quota_hosts = hosts;
        cfg.quota_port = port;
        cfg.quota_timeout = 2;
        cfg
    }

    #[test]
    fn probe_sums_answers_and_skips_refusals() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("quota_current.txt"),
            "jay|1M|jay@example.org\n",
        )
        .expect("seed");

        // Every host shares one port, so "two daemons" is the same listener
        // named twice: two answers of 123 each.
        let (port, handle) = spawn_daemon("OK 123\n", 2);
        let cfg = test_config(
            dir.path(),
            vec!["127.0.0.1".to_string(), "127.0.0.1".to_string()],
            port,
        );
        let snap = probe(&cfg, "jay", 4242).expect("probe");
        let requests = handle.join().expect("daemon thread");
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r == "QUOTA 4242\n"), "{requests:?}");
        assert_eq!(snap.user, "jay");
        assert_eq!(snap.used_kb, 246);
        assert_eq!(snap.hard_kb, 1024);
        assert_eq!(snap.over_kb, 0);
        assert_eq!(snap.pct, 24);
        assert!(!snap.over);
        assert!(snap.checked_epoch > 0);
    }

    #[test]
    fn used_kb_partial_sum_when_one_host_refuses() {
        let (port, handle) = spawn_daemon("OK 1000\n", 1);
        // The listener is bound to 127.0.0.1 only, so the same port on
        // 127.0.0.2 (also loopback on Linux) is refused at once. The refusing
        // host goes first to prove the loop carries on past it.
        let mut cfg = Config::defaults();
        cfg.quota_hosts = vec!["127.0.0.2".to_string(), "127.0.0.1".to_string()];
        cfg.quota_port = port;
        cfg.quota_timeout = 2;
        let total = used_kb(&cfg, 7).expect("one answer suffices");
        let _ = handle.join();
        assert_eq!(total, 1000);
    }

    #[test]
    fn probe_fails_when_nothing_answers() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("quota_current.txt"),
            "jay|1M|jay@example.org\n",
        )
        .expect("seed");
        let cfg = test_config(dir.path(), vec!["127.0.0.1".to_string()], closed_port());
        let err = probe(&cfg, "jay", 1).expect_err("no daemon");
        assert!(err.to_string().contains("no quota daemon"), "{err}");
    }

    #[test]
    fn probe_fails_without_a_file_entry_before_touching_the_network() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Unroutable host with a long timeout: if the network half ran, this
        // test would take 30 s.
        let mut cfg = test_config(dir.path(), vec!["10.255.255.1".to_string()], 9878);
        cfg.quota_timeout = 30;
        let err = probe(&cfg, "jay", 1).expect_err("missing file");
        assert!(err.to_string().contains("quota file"), "{err}");

        fs::write(
            dir.path().join("quota_current.txt"),
            "jayne|1T|x\nzero|0|x\n",
        )
        .expect("seed");
        let err = probe(&cfg, "jay", 1).expect_err("no entry");
        assert!(err.to_string().contains("no quota entry"), "{err}");
        let err = probe(&cfg, "zero", 1).expect_err("zero limit");
        assert!(err.to_string().contains("zero limit"), "{err}");
    }

    #[test]
    fn daemon_garbage_is_not_an_answer() {
        let (port, handle) = spawn_daemon("NOPE\n", 1);
        let mut cfg = Config::defaults();
        cfg.quota_hosts = vec!["127.0.0.1".to_string()];
        cfg.quota_port = port;
        cfg.quota_timeout = 2;
        assert!(used_kb(&cfg, 7).is_err());
        let _ = handle.join();
    }

    #[test]
    fn cache_round_trip_and_key_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().join("cache"));
        assert_eq!(cached(&sd), None);
        let snap = snapshot("jay", 537185280, 524288000, 1783152195);
        write_cache(&sd, &snap).expect("write");
        let text = fs::read_to_string(sd.quota_file()).expect("read");
        assert_eq!(
            text,
            "{\"user\":\"jay\",\"used_kb\":537185280,\"hard_kb\":524288000,\"over_kb\":12897280,\"pct\":102,\"over\":true,\"checked_epoch\":1783152195}\n"
        );
        assert_eq!(cached(&sd), Some(snap));
        assert!(!dir.path().join("cache").join("quota.json.tmp").exists());
    }

    #[test]
    fn cache_reads_the_0x_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().to_path_buf());
        fs::write(
            sd.quota_file(),
            "{\"user\":\"jay\",\"used_kb\":537185280,\"hard_kb\":524288000,\"over_kb\":12897280,\"pct\":102.5,\"over\":1,\"checked_epoch\":1783152195}\n",
        )
        .expect("seed");
        let q = cached(&sd).expect("parse 0.x");
        assert_eq!(q.pct, 102);
        assert!(q.over);
        fs::write(sd.quota_file(), "{\"user\":\"jay\",\"used_kb\":1,\"hard_kb\":2,\"over_kb\":0,\"pct\":50.0,\"over\":0,\"checked_epoch\":1}").expect("seed");
        let q = cached(&sd).expect("parse 0.x not over");
        assert!(!q.over);
        assert_eq!(q.pct, 50);
        fs::write(sd.quota_file(), "garbage").expect("seed");
        assert_eq!(cached(&sd), None);
    }
}
