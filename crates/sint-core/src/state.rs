//! `~/.cache/sinteractive/` state files.
//!
//! | File | Purpose |
//! |---|---|
//! | `<jobid>.json` | time-budget snapshot, **frozen schema and field order** |
//! | `<jobid>.poke` | touch to force the in-session loop to re-query now |
//! | `<jobid>.notices` | see [`crate::notices`] |
//! | `quota.json` | see [`crate::quota`] |
//! | `<jobid>.metrics.json` | phase 3: latest host snapshot |
//! | `<jobid>.events.ndjson` | phase 3: event log |
//!
//! Every write is write-to-`.tmp`-then-rename so reads are atomic.
//!
//! Honesty contract: `<jobid>.json` is written only when the deadline was
//! confirmed against Slurm just now; if Slurm is unreachable the file is left
//! alone so it ages truthfully. Consumers age it as
//! `remaining_seconds - (now - updated_epoch)` and treat > 120 s as stale.
//! **Never add a key ending in `name` after `name`** — the 0.x bash completion
//! parses it with a greedy regex.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StateFile {
    pub job_id: u64,
    pub name: Option<String>,
    pub node: String,
    pub end_epoch: Option<i64>,
    pub remaining_seconds: Option<i64>,
    pub updated_epoch: i64,
}

/// Seconds after which a state file is stale.
pub const STALE_AFTER: i64 = 120;

impl StateFile {
    /// `remaining_seconds - (now - updated_epoch)`, clamped ≥ 0; `None` when
    /// stale or when the file carries no deadline.
    ///
    /// Matches the walltime-guard hook: an age outside `0..=120` — including
    /// a negative one from clock skew — means "don't know", not "plenty".
    pub fn aged_remaining(&self, now: i64) -> Option<i64> {
        let remaining = self.remaining_seconds?;
        let age = now - self.updated_epoch;
        if !(0..=STALE_AFTER).contains(&age) {
            return None;
        }
        Some((remaining - age).max(0))
    }
}

/// Paths under the cache dir.
#[derive(Debug, Clone)]
pub struct StateDir(pub PathBuf);

impl StateDir {
    pub fn state_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.json"))
    }
    pub fn poke_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.poke"))
    }
    pub fn notices_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.notices"))
    }
    pub fn quota_file(&self) -> PathBuf {
        self.0.join("quota.json")
    }
    pub fn metrics_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.metrics.json"))
    }
    pub fn events_file(&self, job_id: u64) -> PathBuf {
        self.0.join(format!("{job_id}.events.ndjson"))
    }

    /// Read and parse `<jobid>.json`; `None` when absent or unparseable.
    pub fn read_state(&self, job_id: u64) -> Option<StateFile> {
        let text = fs::read_to_string(self.state_file(job_id)).ok()?;
        serde_json::from_str(&text).ok()
    }
    /// Write `<jobid>.json` atomically, one line plus a trailing newline.
    pub fn write_state(&self, s: &StateFile) -> io::Result<()> {
        let mut body = serde_json::to_string(s).map_err(io::Error::other)?;
        body.push('\n');
        atomic_write(&self.state_file(s.job_id), body.as_bytes())
    }
    /// Touch `<jobid>.poke`.
    pub fn poke(&self, job_id: u64) -> io::Result<()> {
        fs::create_dir_all(&self.0)?;
        fs::File::create(self.poke_file(job_id)).map(|_| ())
    }
    /// Consume a poke: returns true (and removes the file) if one was pending.
    pub fn take_poke(&self, job_id: u64) -> bool {
        fs::remove_file(self.poke_file(job_id)).is_ok()
    }
    /// Poke every `<jobid>.json` present (skips `quota`). Used by `quota --check`.
    pub fn poke_all(&self) -> io::Result<()> {
        for id in self.known_job_ids() {
            self.poke(id)?;
        }
        Ok(())
    }
    /// Remove every per-job file for `job_id` (json, tmp, poke, notices, metrics, events).
    pub fn cleanup(&self, job_id: u64) {
        let files = [
            self.state_file(job_id),
            self.poke_file(job_id),
            self.notices_file(job_id),
            self.metrics_file(job_id),
            self.events_file(job_id),
        ];
        for f in &files {
            let _ = fs::remove_file(f);
            let _ = fs::remove_file(tmp_path(f));
        }
    }
    /// Job ids that have a state file (for completion and `poke_all`), sorted.
    /// Only `<digits>.json` counts, which leaves out `quota.json` and
    /// `<jobid>.metrics.json` by construction.
    pub fn known_job_ids(&self) -> Vec<u64> {
        let Ok(entries) = fs::read_dir(&self.0) else {
            return Vec::new();
        };
        let mut ids: Vec<u64> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension()? != "json" {
                    return None;
                }
                path.file_stem()?.to_str()?.parse::<u64>().ok()
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// `<path>.tmp` — the sibling the script used, so its cleanup lists still apply.
fn tmp_path(path: &Path) -> PathBuf {
    let mut os: OsString = path.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

/// Write `contents` to `path` via a sibling `.tmp` and rename. Creates the
/// parent directory if needed. The file ends up mode 0644.
pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = tmp_path(path);
    let result = (|| -> io::Result<()> {
        fs::write(&tmp, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644))?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "{\"job_id\":147845,\"name\":\"mywork\",\"node\":\"compute20\",\"end_epoch\":1783180952,\"remaining_seconds\":28757,\"updated_epoch\":1783152195}\n";

    fn sample() -> StateFile {
        StateFile {
            job_id: 147845,
            name: Some("mywork".to_string()),
            node: "compute20".to_string(),
            end_epoch: Some(1783180952),
            remaining_seconds: Some(28757),
            updated_epoch: 1783152195,
        }
    }

    #[test]
    fn aged_remaining_ages_and_goes_stale() {
        let s = sample();
        let t0 = s.updated_epoch;
        assert_eq!(s.aged_remaining(t0), Some(28757));
        assert_eq!(s.aged_remaining(t0 + 119), Some(28757 - 119));
        assert_eq!(s.aged_remaining(t0 + 120), Some(28757 - 120));
        assert_eq!(s.aged_remaining(t0 + 121), None, "stale");
        assert_eq!(
            s.aged_remaining(t0 - 1),
            None,
            "clock skew is unknown, not fresh"
        );

        let mut nearly_done = s.clone();
        nearly_done.remaining_seconds = Some(30);
        assert_eq!(nearly_done.aged_remaining(t0 + 100), Some(0), "clamped");

        let mut no_deadline = s;
        no_deadline.end_epoch = None;
        no_deadline.remaining_seconds = None;
        assert_eq!(no_deadline.aged_remaining(t0), None);
    }

    #[test]
    fn reads_the_0x_fixture_verbatim() {
        let parsed: StateFile = serde_json::from_str(FIXTURE).expect("parse");
        assert_eq!(parsed, sample());

        let nulls = "{\"job_id\":7,\"name\":null,\"node\":\"n1\",\"end_epoch\":null,\"remaining_seconds\":null,\"updated_epoch\":5}";
        let parsed: StateFile = serde_json::from_str(nulls).expect("parse nulls");
        assert_eq!(parsed.name, None);
        assert_eq!(parsed.end_epoch, None);
        assert_eq!(parsed.remaining_seconds, None);
    }

    #[test]
    fn write_state_matches_the_0x_bytes_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().join("nested").join("sinteractive"));
        sd.write_state(&sample()).expect("write");
        let path = sd.state_file(147845);
        assert_eq!(fs::read_to_string(&path).expect("read"), FIXTURE);
        assert!(!tmp_path(&path).exists(), "no .tmp left behind");
        assert_eq!(sd.read_state(147845), Some(sample()));
        assert_eq!(sd.read_state(1), None);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }
    }

    #[test]
    fn read_state_tolerates_garbage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().to_path_buf());
        fs::write(sd.state_file(3), "{not json").expect("write");
        assert_eq!(sd.read_state(3), None);
    }

    #[test]
    fn atomic_write_overwrites_in_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.txt");
        atomic_write(&path, b"one\n").expect("first");
        atomic_write(&path, b"two\n").expect("second");
        assert_eq!(fs::read_to_string(&path).expect("read"), "two\n");
        assert_eq!(fs::read_dir(dir.path()).expect("ls").count(), 1);
    }

    #[test]
    fn poke_take_and_poke_all_skip_quota() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().to_path_buf());
        assert!(!sd.take_poke(10), "nothing pending");
        sd.poke(10).expect("poke");
        assert!(sd.poke_file(10).exists());
        assert!(sd.take_poke(10));
        assert!(!sd.poke_file(10).exists());
        assert!(!sd.take_poke(10), "consumed");

        for id in [10u64, 20, 30] {
            let mut s = sample();
            s.job_id = id;
            sd.write_state(&s).expect("write");
        }
        fs::write(sd.quota_file(), "{}").expect("quota");
        fs::write(sd.metrics_file(20), "{}").expect("metrics");
        fs::write(sd.0.join("notes.json"), "{}").expect("other");
        assert_eq!(sd.known_job_ids(), vec![10, 20, 30]);

        sd.poke_all().expect("poke_all");
        for id in [10u64, 20, 30] {
            assert!(sd.poke_file(id).exists(), "{id} poked");
        }
        assert!(!sd.0.join("quota.poke").exists());
        assert!(!sd.0.join("notes.poke").exists());
        assert!(!sd.0.join("20.metrics.poke").exists());
    }

    #[test]
    fn known_job_ids_on_a_missing_dir_is_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().join("absent"));
        assert!(sd.known_job_ids().is_empty());
        assert!(sd.poke_all().is_ok());
    }

    #[test]
    fn cleanup_removes_every_per_job_file_and_nothing_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sd = StateDir(dir.path().to_path_buf());
        let id = 555;
        let mine = [
            sd.state_file(id),
            tmp_path(&sd.state_file(id)),
            sd.poke_file(id),
            sd.notices_file(id),
            tmp_path(&sd.notices_file(id)),
            sd.metrics_file(id),
            sd.events_file(id),
        ];
        let others = [sd.quota_file(), sd.state_file(556), sd.notices_file(556)];
        for f in mine.iter().chain(others.iter()) {
            fs::write(f, "x").expect("seed");
        }
        sd.cleanup(id);
        for f in &mine {
            assert!(!f.exists(), "{} should be gone", f.display());
        }
        for f in &others {
            assert!(f.exists(), "{} should survive", f.display());
        }
        // Idempotent on an empty dir.
        sd.cleanup(id);
    }
}
