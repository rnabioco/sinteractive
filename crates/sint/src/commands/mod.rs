//! Subcommand implementations. Each returns the process exit code.
//!
//! Exit codes follow 0.x: 0 success, 1 not found / failure, 2 usage.

use anyhow::Result;

use crate::cli::Command;

pub mod agent_context;
pub mod attach;
pub mod attach_local;
pub mod cancel;
pub mod common;
pub mod doctor;
pub mod ensure;
pub mod hook;
pub mod install_claude;
pub mod job;
pub mod launch;
pub mod list;
pub mod monitor;
pub mod monitor_tui;
pub mod peek;
pub mod popup;
pub mod queue;
pub mod quota;
pub mod send;
pub mod snapshot;
pub mod status;
pub mod statusline;

/// Everything not yet implemented in this phase.
fn not_yet(what: &str) -> Result<i32> {
    anyhow::bail!("`{what}` is not implemented yet")
}

pub fn dispatch(command: Command) -> Result<i32> {
    match command {
        Command::Launch(args) => launch::run(args),
        Command::Attach(args) => attach::run(args),
        Command::Ensure(args) => ensure::run(args),
        Command::Status(args) => status::run(args, false),
        Command::Refresh(args) => status::run(args, true),
        Command::List(args) => list::run(args),
        Command::Cancel(args) => cancel::run(args),
        Command::AgentContext => agent_context::run(),
        Command::Quota(args) => quota::run(args),
        Command::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = crate::cli::Cli::command();
            clap_complete::generate(shell, &mut cmd, "sinteractive", &mut std::io::stdout());
            Ok(0)
        }
        Command::Man => {
            use clap::CommandFactory;
            let cmd = crate::cli::Cli::command();
            clap_mangen::Man::new(cmd).render(&mut std::io::stdout())?;
            Ok(0)
        }
        Command::Schema => {
            let schema = serde_json::json!({
                "session": schemars::schema_for!(sint_core::session::SessionInfo),
                "state_file": schemars::schema_for!(sint_core::state::StateFile),
                "quota": schemars::schema_for!(sint_core::quota::QuotaSnapshot),
                "notice": schemars::schema_for!(sint_core::notices::Notice),
            });
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(0)
        }
        // Intercepted in main.rs before clap runs; unreachable here.
        Command::Zellij(args) => {
            let mut zargs = vec!["zellij".to_string()];
            zargs.extend(args);
            crate::zellij_embed::run(zargs)
        }
        Command::Queue(args) => queue::run(args),
        Command::Monitor(args) => monitor::run(args),
        Command::Snapshot(args) => snapshot::run(args),
        Command::Events(_) => not_yet("events"),
        Command::Peek(args) => peek::run(args),
        Command::Send(args) => send::run(args),
        Command::Hook(args) => hook::run(args),
        Command::Statusline => statusline::run(),
        Command::Mcp => not_yet("mcp"),
        Command::InstallClaude => install_claude::run(),
        Command::Doctor(args) => doctor::run(args),
        Command::Job(args) => job::run(args),
        Command::AttachLocal { session } => attach_local::run(&session),
        Command::Popup { view, job_id } => popup::run(view, job_id),
    }
}
