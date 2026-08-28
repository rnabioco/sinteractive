//! Subcommand implementations. Each returns the process exit code.
//!
//! Exit codes follow 0.x: 0 success, 1 not found / failure, 2 usage.

use anyhow::Result;

use crate::cli::{ClaudeCommand, Command, GenCommand, SessionCommand};

pub mod agent_context;
pub mod attach;
pub mod attach_local;
pub mod cancel;
pub mod common;
pub mod doctor;
pub mod ensure;
pub mod events;
pub mod hook;
pub mod install_claude;
pub mod job;
pub mod launch;
pub mod list;
pub mod mcp;
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

pub fn dispatch(command: Command) -> Result<i32> {
    match command {
        Command::Launch(args) => launch::run(args),
        Command::Attach(args) => attach::run(args),
        Command::List(args) => list::run(args),
        Command::Status(args) => {
            let refresh = args.refresh;
            status::run(args, refresh)
        }
        Command::Cancel(args) => cancel::run(args),
        Command::Queue(args) => queue::run(args),
        Command::Monitor(args) => monitor::run(args),
        Command::Quota(args) => quota::run(args),
        Command::Doctor(args) => doctor::run(args),
        Command::Session(c) => session(c.command),
        Command::Claude(c) => claude(c.command),
        Command::Gen(g) => generate(g.command),
        // Intercepted in main.rs before clap runs; unreachable here.
        Command::Zellij(args) => {
            let mut zargs = vec!["zellij".to_string()];
            zargs.extend(args);
            crate::zellij_embed::run(zargs)
        }

        // Hidden aliases for the pre-grouping names, onto the same bodies.
        Command::Ensure(args) => session(SessionCommand::Ensure(args)),
        Command::Peek(args) => session(SessionCommand::Peek(args)),
        Command::Send(args) => session(SessionCommand::Send(args)),
        Command::Events(args) => session(SessionCommand::Events(args)),
        Command::Refresh(args) => status::run(args, true),
        Command::Snapshot(args) => snapshot::run(args),
        Command::AgentContext => claude(ClaudeCommand::Context),
        Command::Hook(args) => claude(ClaudeCommand::Hook(args)),
        Command::Statusline => claude(ClaudeCommand::Statusline),
        Command::Mcp => claude(ClaudeCommand::Mcp),
        Command::InstallClaude => claude(ClaudeCommand::Install),
        Command::Completions { shell } => generate(GenCommand::Completions { shell }),
        Command::Man => generate(GenCommand::Man),
        Command::Schema => generate(GenCommand::Schema),

        Command::Job(args) => job::run(args),
        Command::AttachLocal { session } => attach_local::run(&session),
        Command::Popup { view, job_id } => popup::run(view, job_id),
    }
}

/// `sinteractive session …`
fn session(command: SessionCommand) -> Result<i32> {
    match command {
        SessionCommand::Ensure(args) => ensure::run(args),
        SessionCommand::Peek(args) => peek::run(args),
        SessionCommand::Send(args) => send::run(args),
        SessionCommand::Events(args) => events::run(args),
    }
}

/// `sinteractive claude …`
fn claude(command: ClaudeCommand) -> Result<i32> {
    match command {
        ClaudeCommand::Install => install_claude::run(),
        ClaudeCommand::Context => agent_context::run(),
        ClaudeCommand::Hook(args) => hook::run(args),
        ClaudeCommand::Statusline => statusline::run(),
        ClaudeCommand::Mcp => mcp::run(),
    }
}

/// `sinteractive gen …`
fn generate(command: GenCommand) -> Result<i32> {
    match command {
        GenCommand::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = crate::cli::Cli::command();
            clap_complete::generate(shell, &mut cmd, "sinteractive", &mut std::io::stdout());
            Ok(0)
        }
        GenCommand::Man => {
            use clap::CommandFactory;
            let cmd = crate::cli::Cli::command();
            clap_mangen::Man::new(cmd).render(&mut std::io::stdout())?;
            Ok(0)
        }
        GenCommand::Schema => {
            let schema = serde_json::json!({
                "session": schemars::schema_for!(sint_core::session::SessionInfo),
                "state_file": schemars::schema_for!(sint_core::state::StateFile),
                "quota": schemars::schema_for!(sint_core::quota::QuotaSnapshot),
                "notice": schemars::schema_for!(sint_core::notices::Notice),
            });
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(0)
        }
    }
}
