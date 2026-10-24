//! Scratch directories with a manifest: working space that names its owner,
//! says when it was made and can be cleaned by age.
//!
//! `<root>/<topic>/.sint-scratch.json` is what makes a directory ours.
//! [`clean`] touches nothing without one, and nothing that is not a real
//! directory directly under the root, so a stray symlink or a hand-made
//! directory is never deleted by this module.
//!
//! The root is Alpine's `/scratch/alpine/$USER` where that exists, else
//! `~/scratch`. On Bodhi that is on the same quota as home: this makes
//! cleanup targets obvious and expirable; it does not create space.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::state::atomic_write;

pub const MANIFEST: &str = ".sint-scratch.json";

/// The shared-filesystem scratch that files crossing nodes, and throwaway
/// results, go under.
pub fn root(home: &Path, user: &str) -> PathBuf {
    let alpine = Path::new("/scratch/alpine").join(user);
    if alpine.is_dir() {
        return alpine;
    }
    home.join("scratch")
}

/// Letters, digits, `.`, `_`, `-`; not starting with `.` or `-`.
pub fn valid_topic(topic: &str) -> bool {
    !topic.is_empty()
        && topic.len() <= 64
        && !topic.starts_with(['.', '-'])
        && topic
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `14d`, `36h`, `2w` as seconds.
pub fn parse_age(s: &str) -> Option<i64> {
    let (n, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: i64 = n.parse().ok().filter(|n| *n >= 0)?;
    let per = match unit {
        "h" => 3600,
        "d" => 86400,
        "w" => 7 * 86400,
        _ => return None,
    };
    n.checked_mul(per)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Manifest {
    pub topic: String,
    pub owner: String,
    /// The git checkout the directory was made for, and its branch then.
    pub repo: Option<String>,
    pub branch: Option<String>,
    /// The session it was made from.
    pub job_id: Option<u64>,
    pub created_epoch: i64,
    /// [`clean`] leaves the directory alone until then.
    pub keep_until_epoch: Option<i64>,
}

/// Who is making a directory, for its manifest.
#[derive(Debug, Clone, Default)]
pub struct Origin {
    pub owner: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub job_id: Option<u64>,
}

impl Origin {
    /// The owner and session given, the repo and branch of `cwd` if it is in
    /// a git checkout.
    pub fn detect(owner: &str, job_id: Option<u64>, cwd: &Path) -> Self {
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        };
        Origin {
            owner: owner.to_string(),
            repo: git(&["rev-parse", "--show-toplevel"]),
            branch: git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD"),
            job_id,
        }
    }
}

/// Make `<root>/<topic>/` with its manifest, or find it: asking again for a
/// topic that exists returns its path with `false` and changes nothing.
pub fn create(
    root: &Path,
    topic: &str,
    origin: &Origin,
    now: i64,
    keep_for_secs: Option<i64>,
) -> io::Result<(PathBuf, bool)> {
    if !valid_topic(topic) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid topic '{topic}': letters, digits, '.', '_' and '-', not starting with '.' or '-'"),
        ));
    }
    let dir = root.join(topic);
    if dir.join(MANIFEST).is_file() {
        return Ok((dir, false));
    }
    if dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} exists and is not a scratch directory (no {MANIFEST})",
                dir.display()
            ),
        ));
    }
    fs::create_dir_all(&dir)?;
    let manifest = Manifest {
        topic: topic.to_string(),
        owner: origin.owner.clone(),
        repo: origin.repo.clone(),
        branch: origin.branch.clone(),
        job_id: origin.job_id,
        created_epoch: now,
        keep_until_epoch: keep_for_secs.map(|s| now + s),
    };
    let mut body = serde_json::to_string_pretty(&manifest).map_err(io::Error::other)?;
    body.push('\n');
    atomic_write(&dir.join(MANIFEST), body.as_bytes())?;
    Ok((dir, true))
}

pub fn read_manifest(dir: &Path) -> Option<Manifest> {
    serde_json::from_str(&fs::read_to_string(dir.join(MANIFEST)).ok()?).ok()
}

/// Whether `branch` still exists in `repo`; `None` when that cannot be told
/// (no repo recorded, the checkout gone).
pub fn branch_exists(repo: &str, branch: &str) -> Option<bool> {
    if !Path::new(repo).is_dir() {
        return None;
    }
    let out = Command::new("git")
        .args(["-C", repo, "rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .output()
        .ok()?;
    Some(out.status.success())
}

/// Allocated bytes under `path`, hard links counted once per walk.
pub fn dir_bytes(path: &Path) -> u64 {
    fn walk(path: &Path, seen: &mut std::collections::HashSet<(u64, u64)>) -> u64 {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        let mut total = 0;
        for e in entries.flatten() {
            let Ok(meta) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            if meta.is_dir() {
                total += walk(&e.path(), seen);
            } else if meta.is_file() && (meta.nlink() == 1 || seen.insert((meta.dev(), meta.ino())))
            {
                total += meta.blocks() * 512;
            }
        }
        total
    }
    walk(path, &mut Default::default())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Entry {
    pub topic: String,
    pub path: String,
    pub bytes: u64,
    pub age_secs: i64,
    pub owner: String,
    pub branch: Option<String>,
    /// `Some(false)` is the case worth looking at: the branch is gone.
    pub branch_exists: Option<bool>,
    pub job_id: Option<u64>,
    pub keep_until_epoch: Option<i64>,
}

/// The manifested directories directly under `root`, biggest first.
fn manifested(root: &Path) -> Vec<(PathBuf, Manifest)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let meta = fs::symlink_metadata(&path).ok()?;
            if !meta.is_dir() {
                return None;
            }
            let m = read_manifest(&path)?;
            (path.file_name()?.to_str()? == m.topic).then_some((path, m))
        })
        .collect()
}

pub fn list(root: &Path, now: i64) -> Vec<Entry> {
    let mut out: Vec<Entry> = manifested(root)
        .into_iter()
        .map(|(path, m)| Entry {
            bytes: dir_bytes(&path),
            age_secs: (now - m.created_epoch).max(0),
            branch_exists: match (&m.repo, &m.branch) {
                (Some(repo), Some(branch)) => branch_exists(repo, branch),
                _ => None,
            },
            path: path.to_string_lossy().into_owned(),
            topic: m.topic,
            owner: m.owner,
            branch: m.branch,
            job_id: m.job_id,
            keep_until_epoch: m.keep_until_epoch,
        })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.topic.cmp(&b.topic)));
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Cleaned {
    pub topic: String,
    pub path: String,
    pub bytes: u64,
    pub age_secs: i64,
    pub removed: bool,
}

/// The manifested directories created more than `older_than_secs` ago and
/// not held by `keep_until_epoch`; deleted when `delete`, else only listed.
pub fn clean(
    root: &Path,
    older_than_secs: i64,
    now: i64,
    delete: bool,
) -> io::Result<Vec<Cleaned>> {
    let mut out = Vec::new();
    for (path, m) in manifested(root) {
        let age_secs = (now - m.created_epoch).max(0);
        if age_secs < older_than_secs || m.keep_until_epoch.is_some_and(|k| k > now) {
            continue;
        }
        let bytes = dir_bytes(&path);
        if delete {
            fs::remove_dir_all(&path)?;
        }
        out.push(Cleaned {
            topic: m.topic,
            path: path.to_string_lossy().into_owned(),
            bytes,
            age_secs,
            removed: delete,
        });
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.topic.cmp(&b.topic)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86400;

    fn origin() -> Origin {
        Origin {
            owner: "tester".into(),
            branch: Some("feat".into()),
            ..Origin::default()
        }
    }

    #[test]
    fn scratch_is_under_home_where_there_is_no_alpine_scratch() {
        assert_eq!(
            root(Path::new("/beevol/home/x"), "nobody-has-this-scratch-dir"),
            PathBuf::from("/beevol/home/x/scratch")
        );
    }

    #[test]
    fn topics_and_ages_parse() {
        assert!(valid_topic("ecoli_scan-2.v1"));
        for bad in ["", ".hidden", "-x", "a/b", "a b", "..", &"x".repeat(65)] {
            assert!(!valid_topic(bad), "{bad:?}");
        }
        assert_eq!(parse_age("14d"), Some(14 * DAY));
        assert_eq!(parse_age("36h"), Some(36 * 3600));
        assert_eq!(parse_age("2w"), Some(14 * DAY));
        for bad in ["", "d", "14", "-1d", "1.5d", "3m"] {
            assert_eq!(parse_age(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn new_writes_a_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let (dir, created) = create(tmp.path(), "sweep", &origin(), 1000, Some(DAY)).unwrap();
        assert!(created);
        let m = read_manifest(&dir).unwrap();
        assert_eq!(m.topic, "sweep");
        assert_eq!(m.owner, "tester");
        assert_eq!(m.branch.as_deref(), Some("feat"));
        assert_eq!(
            (m.created_epoch, m.keep_until_epoch),
            (1000, Some(1000 + DAY))
        );

        // Asking again finds it and changes nothing.
        let (again, created) = create(tmp.path(), "sweep", &origin(), 9999, None).unwrap();
        assert_eq!((again, created), (dir.clone(), false));
        assert_eq!(read_manifest(&dir).unwrap().created_epoch, 1000);
    }

    #[test]
    fn new_refuses_a_directory_that_is_not_ours_and_a_bad_topic() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("mine")).unwrap();
        let e = create(tmp.path(), "mine", &origin(), 0, None).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        let e = create(tmp.path(), "../x", &origin(), 0, None).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn clean_skips_directories_without_a_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("handmade")).unwrap();
        fs::write(tmp.path().join("handmade/data"), "x").unwrap();
        // A manifest copied into a directory of another name does not count.
        create(tmp.path(), "real", &origin(), 0, None).unwrap();
        fs::create_dir(tmp.path().join("impostor")).unwrap();
        fs::copy(
            tmp.path().join("real").join(MANIFEST),
            tmp.path().join("impostor").join(MANIFEST),
        )
        .unwrap();
        let got = clean(tmp.path(), 0, 100 * DAY, true).unwrap();
        assert_eq!(
            got.iter().map(|c| c.topic.as_str()).collect::<Vec<_>>(),
            ["real"]
        );
        assert!(tmp.path().join("handmade/data").exists());
        assert!(tmp.path().join("impostor").exists());
        assert!(!tmp.path().join("real").exists());
    }

    #[test]
    fn clean_skips_keep_until_and_the_young() {
        let tmp = tempfile::tempdir().unwrap();
        create(tmp.path(), "old", &origin(), 0, None).unwrap();
        create(tmp.path(), "held", &origin(), 0, Some(30 * DAY)).unwrap();
        create(tmp.path(), "young", &origin(), 19 * DAY, None).unwrap();
        let got = clean(tmp.path(), 14 * DAY, 20 * DAY, true).unwrap();
        assert_eq!(
            got.iter().map(|c| c.topic.as_str()).collect::<Vec<_>>(),
            ["old"]
        );
        assert!(tmp.path().join("held").exists());
        assert!(tmp.path().join("young").exists());
        // Once the hold has passed and the young one has aged, both go.
        let got = clean(tmp.path(), 14 * DAY, 34 * DAY, true).unwrap();
        assert_eq!(
            got.iter().map(|c| c.topic.as_str()).collect::<Vec<_>>(),
            ["held", "young"]
        );
    }

    #[test]
    fn clean_dry_run_deletes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (dir, _) = create(tmp.path(), "old", &origin(), 0, None).unwrap();
        fs::write(dir.join("big.bin"), vec![1u8; 64 * 1024]).unwrap();
        let got = clean(tmp.path(), 0, DAY, false).unwrap();
        assert_eq!(got.len(), 1);
        assert!(!got[0].removed);
        assert!(got[0].bytes >= 64 * 1024);
        assert!(dir.join("big.bin").exists());
    }

    #[test]
    fn clean_never_follows_a_symlink_out_of_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        create(elsewhere.path(), "victim", &origin(), 0, None).unwrap();
        std::os::unix::fs::symlink(elsewhere.path().join("victim"), tmp.path().join("victim"))
            .unwrap();
        let got = clean(tmp.path(), 0, DAY, true).unwrap();
        assert!(got.is_empty());
        assert!(elsewhere.path().join("victim").exists());
    }

    #[test]
    fn ls_reports_size_age_and_a_vanished_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .output()
                .unwrap()
                .status
                .success())
        };
        git(&["init", "-q", "-b", "main"]);
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ]);
        let o = Origin {
            owner: "tester".into(),
            repo: Some(repo.path().to_string_lossy().into_owned()),
            branch: Some("main".into()),
            job_id: Some(7),
        };
        let (a, _) = create(tmp.path(), "a", &o, 0, None).unwrap();
        fs::write(a.join("f"), vec![1u8; 32 * 1024]).unwrap();
        let o2 = Origin {
            branch: Some("gone".into()),
            ..o
        };
        create(tmp.path(), "b", &o2, 0, None).unwrap();
        let got = list(tmp.path(), 3 * DAY);
        assert_eq!(
            got.iter().map(|e| e.topic.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(got[0].age_secs, 3 * DAY);
        assert!(got[0].bytes >= 32 * 1024);
        assert_eq!(got[0].branch_exists, Some(true));
        assert_eq!(got[1].branch_exists, Some(false));
        assert_eq!(got[0].job_id, Some(7));
    }
}
