//! The batch job body end to end: `__job` brings up a real headless zellij
//! session (the embedded server, the bundled plugin), writes the readiness
//! marker, the state file, the metrics snapshot and the event log, samples
//! the user's other running job through the fake `ssh`, and tears
//! everything down when the session is killed.
//!
//! Skips (prints and returns) when the plugin is not embedded
//! (`SINT_SKIP_BUNDLE`) or no scratch dir can be made.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{FakeSlurm, Job};

/// The two headless tests each start a zellij server; run them one at a time
/// so the 8-CPU CI slot is not shared between two cold zellij starts.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const JOB_ID: u64 = 4242;
const OTHER_ID: u64 = 4244;
const SESSION: &str = "sinteractive-4242";

/// The child `__job`, killed on drop so a failed assertion cannot leave a
/// zellij server behind.
struct JobProcess {
    child: Child,
    log: PathBuf,
}

impl JobProcess {
    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for JobProcess {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// `YYYY-MM-DDTHH:MM:SS` in UTC, the way the binary reads it under `TZ=UTC`.
fn slurm_stamp(epoch: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(epoch)
        .unwrap()
        .format(&time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]"
        ))
        .unwrap()
}

/// The environment `__job` (and every zellij invocation in this test) runs
/// with: the fixture's, plus the job identity and the per-session zellij
/// settings `zellij_cmd` would derive — never the developer's `ZELLIJ_*`.
fn wire(cmd: &mut Command, fx: &FakeSlurm, runtime: &Path) {
    let socket_dir = runtime.join(format!("sint-{JOB_ID}"));
    cmd.env_clear()
        .env("PATH", fx.path())
        .env("HOME", fx.home_dir())
        .env("USER", "tester")
        .env("FAKE_SLURM_DIR", fx.dir())
        .env("SINTERACTIVE_CACHE", fx.cache_dir())
        .env("CLAUDE_CONFIG_DIR", fx.claude_dir())
        .env("SINTERACTIVE_COLOR", "never")
        .env("SINTERACTIVE_RUNTIME_DIR", runtime)
        .env("SINTERACTIVE_POLL_FAST", "0.2")
        .env(
            "SINTERACTIVE_QUOTA_FILE",
            fx.tmp.path().join("no-quota-file"),
        )
        .env("TZ", "UTC")
        .env("ZELLIJ_SOCKET_DIR", &socket_dir)
        .env("XDG_CACHE_HOME", fx.cache_dir().join("xdg"))
        .env("ZELLIJ_SESSION_NAME", SESSION)
        .current_dir(fx.home_dir());
}

fn zellij(fx: &FakeSlurm, runtime: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sinteractive"));
    cmd.arg("zellij").args(args);
    wire(&mut cmd, fx, runtime);
    cmd.stdin(Stdio::null())
        .output()
        .expect("run the embedded zellij")
}

fn wait_for(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("timed out waiting for {what}");
    false
}

#[test]
fn job_brings_up_a_session_and_tears_it_down() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    if option_env!("SINT_SKIP_BUNDLE").is_some() {
        println!("skipping: built with SINT_SKIP_BUNDLE, no plugin embedded");
        return;
    }
    let Ok(scratch) = tempfile::tempdir() else {
        println!("skipping: no writable temp dir");
        return;
    };
    // Unix socket paths are short; keep the runtime dir directly under the
    // temp root rather than inside the (longer) fixture path.
    let runtime = scratch.path().join("rt");
    fs::create_dir_all(&runtime).expect("runtime dir");

    let fx = FakeSlurm::with_jobs(&[
        Job::new(JOB_ID, "sinteractive:t")
            .node("fakenode01")
            .end_time(&slurm_stamp(now_epoch() + 3600)),
        Job::new(4243, "sinteractive").state("PENDING"),
        // Another running job on another node: sampled over the fake ssh.
        Job::new(OTHER_ID, "").node("fakenode[02-03]"),
    ]);

    let log = scratch.path().join("job.log");
    let logfile = fs::File::create(&log).expect("log file");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sinteractive"));
    cmd.args(["__job", "--session-name", "t"]);
    wire(&mut cmd, &fx, &runtime);
    cmd.env("SLURM_JOB_ID", JOB_ID.to_string())
        .env("SLURM_JOB_NODELIST", "fakenode01")
        .env("SLURM_NTASKS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(logfile.try_clone().expect("dup")))
        .stderr(Stdio::from(logfile));
    let mut job = JobProcess {
        child: cmd.spawn().expect("spawn __job"),
        log,
    };

    let socket_dir = runtime.join(format!("sint-{JOB_ID}"));
    let ready = socket_dir.join("ready");
    let up = wait_for("the ready marker", Duration::from_secs(30), || {
        ready.exists() || matches!(job.child.try_wait(), Ok(Some(_)))
    });
    assert!(
        up && ready.exists(),
        "ready marker {} never appeared; __job said:\n{}",
        ready.display(),
        job.log()
    );
    let stamp: i64 = fs::read_to_string(&ready)
        .expect("read marker")
        .trim()
        .parse()
        .expect("epoch in the marker");
    assert!(stamp > 0);
    assert!(
        socket_dir.join("config").exists(),
        "config marker for __attach"
    );

    let out = zellij(&fx, &runtime, &["list-sessions", "--no-formatting"]);
    let listed = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && listed.lines().any(|l| l.starts_with(SESSION)),
        "session not listed: {listed} {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The state file: written from a deadline confirmed by (the fake)
    // squeue on the first tick.
    let state = fx.cache_dir().join(format!("{JOB_ID}.json"));
    assert!(
        wait_for("the state file", Duration::from_secs(10), || state.exists()),
        "__job said:\n{}",
        job.log()
    );
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state).expect("read state")).expect("state json");
    assert_eq!(json["job_id"], JOB_ID);
    assert_eq!(json["name"], "t");
    assert!(
        json["remaining_seconds"].as_i64().unwrap_or(0) > 0,
        "{json}"
    );
    assert!(
        json["end_epoch"].as_i64().unwrap_or(0) > now_epoch(),
        "{json}"
    );
    assert!(
        !fx.cache_dir().join(format!("{JOB_ID}.notices")).exists(),
        "no notices without quota/maint/claude"
    );

    // The local sampler's snapshot, scoped to this job.
    let metrics = fx.cache_dir().join(format!("{JOB_ID}.metrics.json"));
    assert!(
        wait_for("the metrics file", Duration::from_secs(10), || metrics
            .exists()),
        "__job said:\n{}",
        job.log()
    );
    let snap: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&metrics).expect("read metrics"))
            .expect("metrics json");
    assert_eq!(snap["scope"]["job_id"], JOB_ID, "{snap}");
    assert!(
        snap["host"].as_str().is_some_and(|h| !h.is_empty()),
        "{snap}"
    );
    assert!(snap["ts"].as_i64().unwrap_or(0) > 0, "{snap}");
    assert!(snap["cpu"]["ncpu"].as_u64().unwrap_or(0) > 0, "{snap}");

    // The event log opens with `started`.
    let events = fx.cache_dir().join(format!("{JOB_ID}.events.ndjson"));
    assert!(events.exists(), "events file; __job said:\n{}", job.log());
    let first: serde_json::Value = serde_json::from_str(
        fs::read_to_string(&events)
            .expect("read events")
            .lines()
            .next()
            .expect("one event line"),
    )
    .expect("event json");
    assert_eq!(first["kind"], "started", "{first}");
    assert_eq!(first["job"], JOB_ID, "{first}");
    assert_eq!(first["name"], "t", "{first}");
    assert!(
        first["node"].as_str().is_some_and(|n| !n.is_empty()),
        "{first}"
    );
    assert!(first["ts"].as_i64().unwrap_or(0) > 0, "{first}");

    // The other running job was sampled on its node: the fake ssh ran
    // `snapshot --json --job` here and the loop wrote its snapshot.
    let other = fx.cache_dir().join(format!("{OTHER_ID}.metrics.json"));
    assert!(
        wait_for("the other job's snapshot", Duration::from_secs(30), || {
            other.exists()
        }),
        "__job said:\n{}\nssh calls: {:?}",
        job.log(),
        fx.calls_to("ssh")
    );
    let osnap: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&other).expect("read other metrics"))
            .expect("other metrics json");
    assert_eq!(osnap["scope"]["job_id"], OTHER_ID, "{osnap}");
    let ssh = fx.calls_to("ssh");
    assert!(!ssh.is_empty());
    assert_eq!(&ssh[0][..2], ["-o", "BatchMode=yes"]);
    assert!(ssh[0].contains(&"fakenode02".to_string()), "{ssh:?}");
    assert!(
        ssh[0]
            .last()
            .unwrap()
            .ends_with(&format!(" snapshot --json --job {OTHER_ID}")),
        "{ssh:?}"
    );
    assert!(
        !ssh.iter().any(|c| c.contains(&"fakenode01".to_string())),
        "the pending job and our own node are never sampled over ssh: {ssh:?}"
    );

    // `events` on the login node reads the same log.
    let mut ev = fx.sinteractive();
    ev.args(["events", &JOB_ID.to_string()]);
    let out = ev.assert().success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(
        text.lines()
            .next()
            .is_some_and(|l| l.contains("\"kind\":\"started\"")),
        "{text}"
    );

    // The status pipe reached the plugin (the loop asked squeue for the
    // other-jobs summary too).
    let squeue = fx.calls_to("squeue");
    assert!(
        squeue.iter().any(|c| c.iter().any(|a| a == "--jobs")),
        "deadline query: {squeue:?}"
    );
    assert!(
        squeue.iter().any(|c| c.iter().any(|a| a == "--states")),
        "other-jobs query: {squeue:?}"
    );

    // End the session from outside, as Ctrl+d in the last pane would.
    let out = zellij(&fx, &runtime, &["kill-session", SESSION]);
    assert!(
        out.status.success(),
        "kill-session: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exited = wait_for("__job to exit", Duration::from_secs(10), || {
        matches!(job.child.try_wait(), Ok(Some(_)))
    });
    assert!(exited, "__job still running; it said:\n{}", job.log());
    let status = job.child.wait().expect("wait");
    assert!(
        status.success(),
        "__job exited {status}; it said:\n{}",
        job.log()
    );
    assert!(!state.exists(), "state file removed at teardown");
    assert!(!metrics.exists(), "metrics file removed at teardown");
    assert!(!events.exists(), "events file removed at teardown");
    assert!(!socket_dir.exists(), "socket dir removed at teardown");
}

/// Walltime reached: `__job` ends the session itself (the normal exit path,
/// not Slurm's SIGTERM) and exits 0.
#[test]
fn job_ends_the_session_at_the_grace_line() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    if option_env!("SINT_SKIP_BUNDLE").is_some() {
        println!("skipping: built with SINT_SKIP_BUNDLE, no plugin embedded");
        return;
    }
    let Ok(scratch) = tempfile::tempdir() else {
        println!("skipping: no writable temp dir");
        return;
    };
    let runtime = scratch.path().join("rt");
    fs::create_dir_all(&runtime).expect("runtime dir");
    // 30 s left with a 60 s grace: the first tick is already past the line.
    let fx = FakeSlurm::with_jobs(&[
        Job::new(JOB_ID, "sinteractive").end_time(&slurm_stamp(now_epoch() + 30))
    ]);

    let log = scratch.path().join("job.log");
    let logfile = fs::File::create(&log).expect("log file");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sinteractive"));
    cmd.arg("__job");
    wire(&mut cmd, &fx, &runtime);
    cmd.env("SLURM_JOB_ID", JOB_ID.to_string())
        .env("SINTERACTIVE_GRACE", "60")
        .stdin(Stdio::null())
        .stdout(Stdio::from(logfile.try_clone().expect("dup")))
        .stderr(Stdio::from(logfile));
    let mut job = JobProcess {
        child: cmd.spawn().expect("spawn __job"),
        log,
    };
    let exited = wait_for("__job to end the session", Duration::from_secs(40), || {
        matches!(job.child.try_wait(), Ok(Some(_)))
    });
    assert!(exited, "__job still running; it said:\n{}", job.log());
    let status = job.child.wait().expect("wait");
    assert!(
        status.success(),
        "__job exited {status}; it said:\n{}",
        job.log()
    );
    let socket_dir = runtime.join(format!("sint-{JOB_ID}"));
    assert!(!socket_dir.exists(), "socket dir removed at teardown");
    assert!(!fx.cache_dir().join(format!("{JOB_ID}.json")).exists());
    let out = zellij(&fx, &runtime, &["list-sessions", "--no-formatting"]);
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(SESSION),
        "session killed"
    );
}

#[test]
fn job_refuses_to_run_outside_sbatch() {
    let fx = FakeSlurm::new();
    let mut cmd = fx.sinteractive();
    cmd.arg("__job")
        .assert()
        .failure()
        .stderr(predicates::str::contains("SLURM_JOB_ID"));
}

#[test]
fn attach_local_reports_a_missing_session() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let fx = FakeSlurm::new();
    let mut cmd = fx.sinteractive();
    cmd.args(["__attach", "sinteractive-99"])
        .env("SINTERACTIVE_RUNTIME_DIR", scratch.path())
        .assert()
        .code(1)
        .stderr(predicates::str::contains(
            "session sinteractive-99 not found on this node",
        ));
}
