//! The session pane's live working directory — `list`'s CWD column.
//!
//! zellij has no client-facing "what directory is this pane in" query: the
//! tmux equivalent 0.x used, `display-message -p '#{pane_current_path}'`,
//! has no zellij counterpart (`dump-layout` only ever reports the directory
//! a pane was *opened* with, not where a later `cd` left it). The only place
//! the live answer exists is `/proc/<pid>/cwd` of the pane's own shell
//! process, so this reads it directly from the job's cgroup.

use std::collections::BTreeSet;
use std::path::Path;

use super::cgroup::{JobCgroup, CGROUP_ROOT};

/// The shell every sinteractive pane starts
/// (`assets/zellij/config.kdl`'s `default_shell`). Kept in sync by hand; a
/// mismatch just means the CWD column reports nothing, same as before this
/// existed.
pub const PANE_SHELL: &str = "bash";

/// One candidate process for [`pick_pane_shell`]: enough of
/// `/proc/<pid>/stat` to tell the pane's own shell from a subshell it later
/// ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCandidate {
    pub pid: u32,
    pub comm: String,
    pub starttime: u64,
}

/// The pane's own shell among a set of candidate processes: the oldest one
/// named [`PANE_SHELL`]. The pane's shell starts once, at session creation;
/// anything a user later runs inside it that shares the name — a subshell,
/// `bash script.sh`, a Makefile recipe — starts later and so has a later
/// `starttime`.
pub fn pick_pane_shell(candidates: impl IntoIterator<Item = ShellCandidate>) -> Option<u32> {
    candidates
        .into_iter()
        .filter(|c| c.comm == PANE_SHELL)
        .min_by_key(|c| c.starttime)
        .map(|c| c.pid)
}

fn stat_candidates(pids: &BTreeSet<u32>) -> Vec<ShellCandidate> {
    pids.iter()
        .filter_map(|&pid| {
            let stat = procfs::process::Process::new(pid as i32)
                .ok()?
                .stat()
                .ok()?;
            Some(ShellCandidate {
                pid,
                comm: stat.comm,
                starttime: stat.starttime,
            })
        })
        .collect()
}

/// [`pick_pane_shell`]'s pid, read live from `/proc` for every pid in
/// `pids` (normally a [`JobCgroup::pids`]).
pub fn pane_shell_pid(pids: &BTreeSet<u32>) -> Option<u32> {
    pick_pane_shell(stat_candidates(pids))
}

/// A job's pane's live working directory, with the cgroup mount point made
/// explicit (tests; see [`super::Sampler::with_cgroup_root`] for the same
/// pattern). `None` when the job's cgroup or its shell cannot be found —
/// a finished session, no permission, an unsupported cgroup layout.
pub fn session_cwd_in(root: &Path, job_id: u64, uid: u32) -> Option<String> {
    let cgroup = JobCgroup::find(root, job_id, uid)?;
    let pid = pane_shell_pid(&cgroup.pids())?;
    let cwd = procfs::process::Process::new(pid as i32).ok()?.cwd().ok()?;
    cwd.to_str().map(str::to_string)
}

/// [`session_cwd_in`] against the real cgroup mount, for this user.
pub fn session_cwd(job_id: u64) -> Option<String> {
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    session_cwd_in(Path::new(CGROUP_ROOT), job_id, uid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn cand(pid: u32, comm: &str, starttime: u64) -> ShellCandidate {
        ShellCandidate {
            pid,
            comm: comm.to_string(),
            starttime,
        }
    }

    #[test]
    fn picks_the_oldest_bash() {
        let candidates = [
            cand(200, "bash", 500),
            cand(100, "bash", 300),
            cand(300, "vim", 100),
        ];
        assert_eq!(pick_pane_shell(candidates), Some(100));
    }

    #[test]
    fn ignores_non_shell_processes() {
        let candidates = [cand(1, "sleep", 10), cand(2, "python3", 20)];
        assert_eq!(pick_pane_shell(candidates), None);
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(pick_pane_shell(Vec::<ShellCandidate>::new()), None);
    }

    /// A real `bash` child, kept alive with piped stdin so nothing execs
    /// over it, is found through a fake cgroup tree and its real cwd read
    /// back — the whole [`session_cwd_in`] path short of a live cgroup.
    #[test]
    fn finds_a_running_pane_through_a_fake_cgroup() {
        let work = tempfile::tempdir().unwrap();
        let mut child = Command::new("bash")
            .current_dir(work.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn bash");
        let pid = child.id();

        let cgroup_root = tempfile::tempdir().unwrap();
        // v2 (unified) layout: `cgroup.controllers` at the root is what
        // `JobCgroup::find` keys off to pick this branch over v1's
        // per-controller directories.
        fs::write(cgroup_root.path().join("cgroup.controllers"), "").unwrap();
        let job_dir = cgroup_root.path().join("slurm/uid_0/job_555");
        fs::create_dir_all(&job_dir).unwrap();
        fs::write(job_dir.join("cgroup.procs"), format!("{pid}\n")).unwrap();

        let cwd = session_cwd_in(cgroup_root.path(), 555, 0);

        // Tear down before asserting, so a failed assertion never leaks it.
        let _ = child.stdin.take().map(|mut s| s.write_all(b"\n"));
        let _ = child.kill();
        let _ = child.wait();

        assert_eq!(cwd.as_deref(), work.path().canonicalize().unwrap().to_str());
    }

    #[test]
    fn no_cgroup_is_none() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(session_cwd_in(root.path(), 999, 0), None);
    }
}
