//! What is using the quota: a cached index of the largest directories under
//! a root, and of big files that are probably copies of one another.
//!
//! Walking a home directory of ten million files on BeeGFS takes a long
//! time, so nothing here runs from a session or the MCP server. [`build`] is
//! run by an `sbatch` job (`sinteractive quota --reindex`) and writes
//! `usage.json.gz` into the cache; [`load`] and [`report`] only read it.
//!
//! Sizes are allocated bytes (`st_blocks * 512`), which is what the quota
//! counts; a file with several hard links is counted once.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::state::{atomic_write, StateDir};

/// A directory is indexed once its subtree holds this much.
pub const MIN_DIR_BYTES: u64 = 100 << 20;
/// A file is a duplicate candidate from this size up.
pub const BIG_FILE_BYTES: u64 = 1 << 30;
/// An index older than this is reported `stale`.
pub const STALE_AFTER_SECS: i64 = 24 * 3600;
/// A directory is skipped in a report when one child holds this share of it.
const DOMINANT_SHARE: f64 = 0.8;
/// How much of each end of a file the duplicate check reads.
const FINGERPRINT_BYTES: u64 = 1 << 20;

/// What a directory is, as far as cleanup is concerned. A directory inside a
/// `worktree`, `cargo-target` or `cache` directory takes its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Worktree,
    CargoTarget,
    Cache,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DirUsage {
    pub path: String,
    /// Allocated bytes in the whole subtree.
    pub bytes: u64,
    pub files: u64,
    /// Modification time of the newest file in the subtree.
    pub newest_epoch: i64,
    pub kind: Kind,
}

/// Files over [`BIG_FILE_BYTES`] with the same size and the same head and
/// tail: almost certainly copies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DupGroup {
    /// Size of each file.
    pub bytes: u64,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageIndex {
    pub generated_epoch: i64,
    pub root: String,
    /// Every directory over the threshold, ancestors included.
    pub dirs: Vec<DirUsage>,
    pub duplicates: Vec<DupGroup>,
}

/// What `quota --top` and the `quota_top` tool return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageReport {
    pub root: String,
    pub top: Vec<DirUsage>,
    pub duplicates: Vec<DupGroup>,
    /// Seconds since the index was built.
    pub age_secs: i64,
    /// Older than 24 h: the numbers may no longer be true.
    pub stale: bool,
}

/// The two thresholds, so a test can use small files.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub min_dir_bytes: u64,
    pub big_file_bytes: u64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            min_dir_bytes: MIN_DIR_BYTES,
            big_file_bytes: BIG_FILE_BYTES,
        }
    }
}

#[derive(Default)]
struct Totals {
    bytes: u64,
    files: u64,
    newest: i64,
}

struct Walk {
    params: Params,
    dirs: Vec<DirUsage>,
    big: Vec<(u64, String)>,
    seen_links: HashSet<(u64, u64)>,
}

/// The kind of directory `path` is, given its parent's kind.
fn kind_of(path: &Path, parent: Kind) -> Kind {
    if matches!(parent, Kind::CargoTarget | Kind::Cache | Kind::Worktree) {
        return parent;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let under = |dir: &str| {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some(dir)
    };
    if name == "target" && path.parent().is_some_and(|p| p.join("Cargo.toml").exists()) {
        Kind::CargoTarget
    } else if under("worktrees") && path.to_string_lossy().contains("/.claude/worktrees/") {
        Kind::Worktree
    } else if name == ".cache" {
        Kind::Cache
    } else {
        Kind::Other
    }
}

impl Walk {
    fn dir(&mut self, path: &Path, kind: Kind) -> Totals {
        let mut t = Totals::default();
        let Ok(entries) = fs::read_dir(path) else {
            return t;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            let Ok(meta) = fs::symlink_metadata(&child) else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let sub = self.dir(&child, kind_of(&child, kind));
                t.bytes += sub.bytes;
                t.files += sub.files;
                t.newest = t.newest.max(sub.newest);
            } else if ft.is_file() {
                if meta.nlink() > 1 && !self.seen_links.insert((meta.dev(), meta.ino())) {
                    continue;
                }
                let bytes = meta.blocks() * 512;
                t.bytes += bytes;
                t.files += 1;
                t.newest = t.newest.max(meta.mtime());
                if meta.len() >= self.params.big_file_bytes {
                    self.big
                        .push((meta.len(), child.to_string_lossy().into_owned()));
                }
            }
        }
        if t.bytes >= self.params.min_dir_bytes {
            self.dirs.push(DirUsage {
                path: path.to_string_lossy().into_owned(),
                bytes: t.bytes,
                files: t.files,
                newest_epoch: t.newest,
                kind,
            });
        }
        t
    }
}

/// Walk `root` and build the index. Slow on a big tree; see the module docs.
pub fn build(root: &Path, now: i64, params: Params) -> UsageIndex {
    let mut walk = Walk {
        params,
        dirs: Vec::new(),
        big: Vec::new(),
        seen_links: HashSet::new(),
    };
    walk.dir(root, kind_of(root, Kind::Other));
    walk.dirs
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    UsageIndex {
        generated_epoch: now,
        root: root.to_string_lossy().into_owned(),
        dirs: walk.dirs,
        duplicates: duplicates(walk.big),
    }
}

/// Group `(len, path)` pairs by length, then by a hash of the first and last
/// megabyte; groups of one are dropped.
fn duplicates(mut big: Vec<(u64, String)>) -> Vec<DupGroup> {
    big.sort();
    let mut groups: HashMap<(u64, u64), Vec<String>> = HashMap::new();
    let mut i = 0;
    while i < big.len() {
        let mut j = i;
        while j < big.len() && big[j].0 == big[i].0 {
            j += 1;
        }
        if j - i > 1 {
            for (len, path) in &big[i..j] {
                if let Ok(fp) = fingerprint(Path::new(path), *len) {
                    groups.entry((*len, fp)).or_default().push(path.clone());
                }
            }
        }
        i = j;
    }
    let mut out: Vec<DupGroup> = groups
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|((bytes, _), mut paths)| {
            paths.sort();
            DupGroup { bytes, paths }
        })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.paths.cmp(&b.paths)));
    out
}

fn fingerprint(path: &Path, len: u64) -> io::Result<u64> {
    let mut f = fs::File::open(path)?;
    let mut h = DefaultHasher::new();
    let n = FINGERPRINT_BYTES.min(len) as usize;
    let mut buf = vec![0u8; n];
    f.read_exact(&mut buf)?;
    h.write(&buf);
    if len > FINGERPRINT_BYTES {
        f.seek(SeekFrom::Start(len - n as u64))?;
        f.read_exact(&mut buf)?;
        h.write(&buf);
    }
    Ok(h.finish())
}

pub fn write(dir: &StateDir, index: &UsageIndex) -> io::Result<()> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    serde_json::to_writer(&mut enc, index).map_err(io::Error::other)?;
    enc.flush()?;
    atomic_write(&dir.usage_file(), &enc.finish()?)
}

/// The cached index; `None` when there is none or it cannot be read.
pub fn load(dir: &StateDir) -> Option<UsageIndex> {
    let file = fs::File::open(dir.usage_file()).ok()?;
    serde_json::from_reader(GzDecoder::new(file)).ok()
}

/// The `n` directories worth looking at first. A directory whose size is
/// mostly one child is left out in favour of that child, so the list names
/// where the bytes are rather than their parents; the root is left out
/// unless nothing else qualifies.
pub fn report(index: &UsageIndex, n: usize, now: i64) -> UsageReport {
    let dominated = |d: &DirUsage| {
        index.dirs.iter().any(|c| {
            c.bytes as f64 >= DOMINANT_SHARE * d.bytes as f64
                && Path::new(&c.path).parent() == Some(Path::new(&d.path))
        })
    };
    let mut top: Vec<DirUsage> = index
        .dirs
        .iter()
        .filter(|d| !dominated(d))
        .take(n)
        .cloned()
        .collect();
    if top.is_empty() {
        top = index.dirs.iter().take(n).cloned().collect();
    }
    let age_secs = (now - index.generated_epoch).max(0);
    UsageReport {
        root: index.root.clone(),
        top,
        duplicates: index.duplicates.clone(),
        age_secs,
        stale: age_secs > STALE_AFTER_SECS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: Params = Params {
        min_dir_bytes: 8 * 1024,
        big_file_bytes: 32 * 1024,
    };

    fn put(path: &Path, byte: u8, len: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![byte; len]).unwrap();
    }

    #[test]
    fn usage_index_ranks_directories_by_size() {
        let tmp = tempfile::tempdir().unwrap();
        put(&tmp.path().join("big/a.bin"), 1, 200 * 1024);
        put(&tmp.path().join("small/a.bin"), 2, 24 * 1024);
        put(&tmp.path().join("tiny/a.bin"), 3, 10);
        let index = build(tmp.path(), 1000, SMALL);
        let r = report(&index, 10, 1000);
        let names: Vec<&str> = r
            .top
            .iter()
            .map(|d| d.path.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(names, ["big", "small"], "tiny is under the threshold");
        assert!(r.top[0].bytes >= 200 * 1024);
        assert_eq!(r.top[0].files, 1);
    }

    #[test]
    fn a_parent_that_is_mostly_one_child_gives_way_to_it() {
        let tmp = tempfile::tempdir().unwrap();
        put(&tmp.path().join("p/q/a.bin"), 1, 200 * 1024);
        put(&tmp.path().join("p/b.bin"), 2, 10 * 1024);
        let index = build(tmp.path(), 0, SMALL);
        let r = report(&index, 10, 0);
        assert_eq!(r.top.len(), 1, "{:?}", r.top);
        assert!(r.top[0].path.ends_with("/p/q"));
    }

    #[test]
    fn duplicates_need_equal_size_and_checksum() {
        let tmp = tempfile::tempdir().unwrap();
        put(&tmp.path().join("one/x.pod5"), 7, 64 * 1024);
        put(&tmp.path().join("two/x.pod5"), 7, 64 * 1024);
        put(&tmp.path().join("three/y.pod5"), 8, 64 * 1024); // same size, other content
        put(&tmp.path().join("four/z.pod5"), 7, 48 * 1024); // same content, other size
        let index = build(tmp.path(), 0, SMALL);
        assert_eq!(index.duplicates.len(), 1, "{:?}", index.duplicates);
        let g = &index.duplicates[0];
        assert_eq!(g.bytes, 64 * 1024);
        assert_eq!(g.paths.len(), 2);
        assert!(g.paths.iter().all(|p| p.ends_with("/x.pod5")));
    }

    #[test]
    fn hard_links_are_not_duplicates_and_count_once() {
        let tmp = tempfile::tempdir().unwrap();
        put(&tmp.path().join("a/x.bin"), 1, 64 * 1024);
        fs::create_dir_all(tmp.path().join("b")).unwrap();
        fs::hard_link(tmp.path().join("a/x.bin"), tmp.path().join("b/x.bin")).unwrap();
        let index = build(tmp.path(), 0, SMALL);
        assert!(index.duplicates.is_empty());
        let total: u64 = index
            .dirs
            .iter()
            .find(|d| d.path == tmp.path().to_string_lossy())
            .map(|d| d.files)
            .unwrap();
        assert_eq!(total, 1);
    }

    #[test]
    fn kinds_follow_the_path() {
        let tmp = tempfile::tempdir().unwrap();
        put(&tmp.path().join("proj/Cargo.toml"), b'x', 10);
        put(&tmp.path().join("proj/target/debug/a.bin"), 1, 64 * 1024);
        put(&tmp.path().join("notrust/target/a.bin"), 1, 64 * 1024);
        put(
            &tmp.path().join("r/.claude/worktrees/w1/data.bin"),
            1,
            64 * 1024,
        );
        put(&tmp.path().join(".cache/tool/a.bin"), 1, 64 * 1024);
        let index = build(tmp.path(), 0, SMALL);
        let kind = |suffix: &str| {
            index
                .dirs
                .iter()
                .find(|d| d.path.ends_with(suffix))
                .unwrap_or_else(|| panic!("no {suffix}"))
                .kind
        };
        assert_eq!(kind("/proj/target"), Kind::CargoTarget);
        assert_eq!(kind("/proj/target/debug"), Kind::CargoTarget);
        assert_eq!(kind("/notrust/target"), Kind::Other);
        assert_eq!(kind("/worktrees/w1"), Kind::Worktree);
        assert_eq!(kind("/.cache/tool"), Kind::Cache);
    }

    #[test]
    fn the_index_round_trips_through_its_gzip_file() {
        let tmp = tempfile::tempdir().unwrap();
        let state = StateDir(tmp.path().join("cache"));
        fs::create_dir_all(&state.0).unwrap();
        assert_eq!(load(&state), None);
        let index = UsageIndex {
            generated_epoch: 5,
            root: "/h".into(),
            dirs: vec![DirUsage {
                path: "/h/a".into(),
                bytes: 9,
                files: 1,
                newest_epoch: 4,
                kind: Kind::Other,
            }],
            duplicates: vec![],
        };
        write(&state, &index).unwrap();
        assert_eq!(load(&state), Some(index));
        let raw = fs::read(state.usage_file()).unwrap();
        assert_eq!(&raw[..2], &[0x1f, 0x8b], "gzip magic");
    }

    #[test]
    fn a_report_says_when_the_index_is_stale() {
        let index = UsageIndex {
            generated_epoch: 1000,
            root: "/h".into(),
            dirs: vec![],
            duplicates: vec![],
        };
        let fresh = report(&index, 5, 1000 + STALE_AFTER_SECS);
        assert_eq!((fresh.age_secs, fresh.stale), (STALE_AFTER_SECS, false));
        assert!(report(&index, 5, 1001 + STALE_AFTER_SECS).stale);
    }
}
