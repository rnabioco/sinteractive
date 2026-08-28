//! `sinteractive attach [TARGET]` — reattach (script lines 2208-2258).
//!
//! No target: your only running session; with none, say how to start one;
//! with several, print the ready-to-run choices and exit 1. The default
//! path goes through Slurm (`srun --overlap --jobid=ID --pty …`), which
//! needs no ssh access to the node; `--ssh` forces `ssh -X -t NODE …`, the
//! launch path's transport, for X11 forwarding.

use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{anyhow, Result};
use sint_core::session::parse_comment;

use super::common::{current_exe, Ctx};
use crate::cli::AttachArgs;

pub fn run(args: AttachArgs) -> Result<i32> {
    let ctx = Ctx::new();
    let p = ctx.palette(2);
    let (reset, bold, dim, err, id, warn) = (&p.reset, &p.bold, &p.dim, &p.err, &p.id, &p.warn);

    // Attaching from inside a session would nest multiplexers.
    if ctx.inside_session() {
        eprintln!(
            "{err}{bold}Error:{reset}{err} Already inside an sinteractive session. Exit this session first.{reset}"
        );
        return Ok(1);
    }

    let target = match args.target.filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => {
            let running = ctx.running_sessions()?;
            match running.as_slice() {
                [] => {
                    eprintln!(
                        "{err}{bold}sinteractive:{reset}{err} no running sinteractive sessions to attach to.{reset}"
                    );
                    eprintln!("{dim}Start one with 'sinteractive'.{reset}");
                    return Ok(1);
                }
                [one] => one.job_id.to_string(),
                many => {
                    eprintln!(
                        "{warn}{bold}sinteractive:{reset}{warn} you have {} running sessions — pick one:{reset}",
                        many.len()
                    );
                    for r in many {
                        let name = parse_comment(&r.comment)
                            .flatten()
                            .unwrap_or_else(|| r.job_id.to_string());
                        eprintln!(
                            "  sinteractive attach {id}{name:<18}{reset} {dim}# job {} on {}, up {}{reset}",
                            r.job_id, r.node, r.elapsed
                        );
                    }
                    return Ok(1);
                }
            }
        }
    };

    let job_id = match ctx.resolve(Some(&target)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{err}{bold}sinteractive:{reset}{err} {e}{reset}");
            return Ok(1);
        }
    };

    // Verify the job is still running.
    let row = ctx.slurm.job(job_id)?;
    let (state, node) = match &row {
        Some(r) if r.state == "RUNNING" => (r.state.clone(), r.node.clone()),
        Some(r) => (r.state.clone(), String::new()),
        None => ("unknown".to_string(), String::new()),
    };
    if state != "RUNNING" {
        eprintln!(
            "{err}{bold}sinteractive:{reset}{err} job {job_id} is not running (state: {state}){reset}"
        );
        return Ok(1);
    }

    eprintln!("{dim}Reattaching to sinteractive session{reset} {id}{job_id}{reset}{dim}...{reset}");
    let exe = current_exe()?;
    let session = format!("sinteractive-{job_id}");
    let mut cmd = if args.ssh {
        let host = match ctx.slurm.batch_host(job_id)? {
            Some(h) => h,
            None if !node.is_empty() => node,
            None => return Err(anyhow!("job {job_id} has no batch host to ssh to")),
        };
        let mut c = Command::new("ssh");
        c.args(["-X", "-t", &host])
            .arg(&exe)
            .arg("__attach")
            .arg(&session);
        c
    } else {
        let mut c = Command::new("srun");
        c.arg("--overlap")
            .arg(format!("--jobid={job_id}"))
            .arg("--pty")
            .arg(&exe)
            .arg("__attach")
            .arg(&session);
        c
    };
    // exec only returns on failure.
    let e = cmd.exec();
    Err(anyhow!("could not exec {:?}: {e}", cmd.get_program()))
}
