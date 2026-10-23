//! `sinteractive session exec TARGET COMMAND` — run a short command in a new
//! floating pane of a session, wait for it, and bring back its output and
//! exit code. The agent's alternative to typing into the user's live shell.
//!
//! Orchestration-sized work only: the pane shares the session's small
//! allocation, so anything heavy still gets its own `srun`.
//!
//! One ssh does the whole round trip. The pane runs a wrapper that tees the
//! command's output to a file in the session's node-local socket directory
//! and writes the exit code beside it; the remote script waits for that,
//! prints both, closes the pane (unless `--keep`) and removes the files. The
//! file is there because zellij 0.45.1 blanks a held pane's screen once its
//! command exits, so `dump-screen` after the fact returns nothing; while the
//! command runs the pane still shows everything live.

use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{anyhow, Result};
use serde::Serialize;

use super::common::{eprint_error, print_json, ssh_batch, Ctx, RunningSession};
use super::peek::first_stderr_line;
use crate::cli::ExecArgs;
use crate::zellij_cmd::{shell_quote, ZellijEnv};

/// Default and cap for the wait, in seconds.
pub const TIMEOUT_DEFAULT_SECS: u64 = 120;
pub const TIMEOUT_MAX_SECS: u64 = 3600;
/// Output kept: the last this many lines of at most [`OUTPUT_MAX_BYTES`].
pub const OUTPUT_MAX_LINES: usize = 200;
const OUTPUT_MAX_BYTES: usize = 256 * 1024;

/// First line of the remote script's output, ahead of the command's own.
const MARKER: &str = "__SINT_EXEC__";

/// What `run_in_session` returns.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ExecResult {
    pub job_id: u64,
    pub node: String,
    pub pane_id: String,
    /// `null` when the command did not finish within the timeout.
    pub exit_code: Option<i32>,
    /// stdout and stderr, interleaved, the last [`OUTPUT_MAX_LINES`] lines.
    pub output: Vec<String>,
    pub truncated: bool,
    pub timed_out: bool,
}

/// Pane name and file stem unique to this call.
fn unique_token() -> String {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    format!(
        "agent-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// What runs inside the pane: `$1` is the command, `$2` the output file.
const WRAPPER: &str = r#"o=$2; bash -lc "$1" 2>&1 | tee "$o"; rc=${PIPESTATUS[0]}; echo "$rc" > "$o.rc.tmp"; mv "$o.rc.tmp" "$o.rc"; exit "$rc""#;

/// The script the node runs.
fn remote_script(
    env: &ZellijEnv,
    token: &str,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    keep: bool,
) -> String {
    let out = shell_quote(
        &env.socket_dir
            .join(format!("{token}.out"))
            .to_string_lossy(),
    );
    let mut run = vec!["run", "--name", token, "--no-focus", "--floating"];
    if let Some(cwd) = cwd {
        run.extend(["--cwd", cwd]);
    }
    run.extend(["--", "bash", "-c", WRAPPER, "_", command]);
    // The output path is the last wrapper argument; it is already quoted.
    let run = env.remote_argv(run).join(" ");
    let close = env
        .remote_argv(["action", "close-pane", "-p", "\"$id\""])
        .join(" ")
        // remote_argv quotes every word; the pane id must expand.
        .replace("'\"$id\"'", "\"$id\"");
    let polls = timeout_secs.saturating_mul(5);
    let close_step = if keep {
        String::new()
    } else {
        format!("{close} >/dev/null 2>&1; ")
    };
    format!(
        r#"out={out}
rm -f "$out" "$out.rc"
id=$({run} {out}) || exit 1
i=0
while [ ! -f "$out.rc" ] && [ "$i" -lt {polls} ]; do sleep 0.2; i=$((i+1)); done
if [ -f "$out.rc" ]; then timed=0; rc=$(cat "$out.rc"); else timed=1; rc=; fi
echo "{MARKER} $id $timed $rc $(wc -c < "$out" 2>/dev/null || echo 0)"
tail -c {OUTPUT_MAX_BYTES} "$out" 2>/dev/null
{close_step}rm -f "$out" "$out.rc" "$out.rc.tmp"
"#
    )
}

/// The header and output of [`remote_script`] as an [`ExecResult`].
fn parse_remote(job_id: u64, node: &str, stdout: &str) -> Result<ExecResult> {
    let (header, body) = stdout.split_once('\n').unwrap_or((stdout, ""));
    let mut f = header.split_whitespace();
    if f.next() != Some(MARKER) {
        return Err(anyhow!("unexpected reply from {node}: {header:?}"));
    }
    let pane_id = f.next().unwrap_or("").to_string();
    let timed_out = f.next() == Some("1");
    let rest: Vec<&str> = f.collect();
    let (exit_code, total_bytes) = match rest.as_slice() {
        [rc, bytes] => (rc.parse().ok(), bytes.parse().unwrap_or(0)),
        [bytes] => (None, bytes.parse().unwrap_or(0)),
        _ => (None, 0),
    };
    let lines: Vec<&str> = body.lines().map(|l| l.trim_end_matches('\r')).collect();
    let start = lines.len().saturating_sub(OUTPUT_MAX_LINES);
    // `tail -c` can start mid-line; that partial line is not output.
    let cut_bytes = total_bytes > OUTPUT_MAX_BYTES;
    let skip = start.max(usize::from(cut_bytes));
    Ok(ExecResult {
        job_id,
        node: node.to_string(),
        pane_id,
        exit_code,
        output: lines[skip.min(lines.len())..]
            .iter()
            .map(|l| l.to_string())
            .collect(),
        truncated: start > 0 || cut_bytes,
        timed_out,
    })
}

/// Run `command` (`bash -lc`) in a new floating pane of `session` and wait
/// up to `timeout_secs` for it. A command that overruns is killed with its
/// pane and reported `timed_out`, with the output it had produced.
pub fn run_in_session(
    ctx: &Ctx,
    session: &RunningSession,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    keep: bool,
) -> Result<ExecResult> {
    let env = ZellijEnv::new(&ctx.cfg, session.job_id)?;
    let token = unique_token();
    let script = remote_script(&env, &token, command, cwd, timeout_secs, keep);
    let remote = format!("bash -c {}", shell_quote(&script));
    let out = ssh_batch(&session.node, 10, &remote).output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "could not run in session {} on {}: {}",
            session.job_id,
            session.node,
            first_stderr_line(&out)
        ));
    }
    parse_remote(
        session.job_id,
        &session.node,
        &String::from_utf8_lossy(&out.stdout),
    )
}

pub fn run(args: ExecArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let p = ctx.palette(2);
    if args.command.trim().is_empty() {
        eprint_error(&p, "nothing to run: COMMAND is empty");
        return Ok(2);
    }
    let Some(session) = ctx.resolve_running(&args.target)? else {
        return Ok(1);
    };
    let timeout = args.timeout.min(TIMEOUT_MAX_SECS);
    let result = match run_in_session(
        &ctx,
        &session,
        &args.command,
        args.cwd.as_deref(),
        timeout,
        args.keep,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprint_error(&p, &format!("{e:#}"));
            return Ok(1);
        }
    };
    if args.json {
        print_json(&result)?;
    } else {
        for line in &result.output {
            println!("{line}");
        }
        if result.truncated {
            eprintln!("(output truncated to the last {OUTPUT_MAX_LINES} lines)");
        }
    }
    if result.timed_out {
        eprint_error(&p, &format!("timed out after {timeout}s"));
        return Ok(124);
    }
    Ok(result.exit_code.unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn env() -> ZellijEnv {
        ZellijEnv {
            job_id: 7,
            socket_dir: PathBuf::from("/tmp/sint-7"),
            xdg_cache_home: PathBuf::from("/c/xdg"),
            cache_dir: PathBuf::from("/c"),
            exe: PathBuf::from("/opt/sinteractive"),
        }
    }

    #[test]
    fn script_runs_a_named_unfocused_floating_pane() {
        let s = remote_script(
            &env(),
            "agent-1-0",
            "echo 'hi'",
            Some("/work dir"),
            30,
            false,
        );
        assert!(
            s.contains(
                "zellij run --name agent-1-0 --no-focus --floating --cwd '/work dir' -- bash -c "
            ),
            "{s}"
        );
        assert!(s.contains("echo '\\''hi'\\''"), "{s}");
        assert!(s.contains("/tmp/sint-7/agent-1-0.out"), "{s}");
        assert!(s.contains("-lt 150 ]"), "{s}");
        assert!(s.contains("action close-pane -p \"$id\""), "{s}");
        // No cwd, no --cwd; --keep, no close.
        let s = remote_script(&env(), "agent-1-1", "true", None, 1, true);
        assert!(!s.contains("--cwd") && !s.contains("close-pane"), "{s}");
    }

    #[test]
    fn parses_the_header_and_caps_the_output() {
        let r = parse_remote(7, "n1", "__SINT_EXEC__ terminal_4 0 3 12\nhello\r\nerr\n").unwrap();
        assert_eq!(r.pane_id, "terminal_4");
        assert_eq!(r.exit_code, Some(3));
        assert_eq!(r.output, ["hello", "err"]);
        assert!(!r.truncated && !r.timed_out);

        let r = parse_remote(7, "n1", "__SINT_EXEC__ terminal_4 1  5\npart\n").unwrap();
        assert!(r.timed_out);
        assert_eq!(r.exit_code, None);
        assert_eq!(r.output, ["part"]);

        let many: String = (0..300).map(|i| format!("{i}\n")).collect();
        let r = parse_remote(
            7,
            "n1",
            &format!("__SINT_EXEC__ terminal_4 0 0 900\n{many}"),
        )
        .unwrap();
        assert_eq!(r.output.len(), OUTPUT_MAX_LINES);
        assert_eq!(r.output.last().map(String::as_str), Some("299"));
        assert!(r.truncated);

        assert!(parse_remote(7, "n1", "bash: oops\n").is_err());
    }

    #[test]
    fn tokens_are_unique() {
        assert_ne!(unique_token(), unique_token());
    }
}
