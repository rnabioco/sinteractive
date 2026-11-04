//! Subcommand implementations. Each returns the process exit code.
//!
//! Exit codes follow 0.x: 0 success, 1 not found / failure, 2 usage.

use anyhow::Result;
use std::io::Write;

use crate::cli::{ClaudeCommand, Command, GenCommand, SessionCommand};

pub mod agent_context;
pub mod attach;
pub mod attach_local;
pub mod cancel;
pub mod common;
pub mod doctor;
pub mod ensure;
pub mod events;
pub mod exec;
pub mod hook;
pub mod install_claude;
pub mod job;
pub mod jobwait;
pub mod launch;
pub mod list;
pub mod mcp;
pub mod monitor;
pub mod monitor_tui;
pub mod pane;
pub mod peek;
pub mod popup;
pub mod queue;
pub mod quota;
pub mod scratch;
pub mod scratch_guard;
pub mod send;
pub mod snapshot;
pub mod status;
pub mod statusline;
pub mod worktree;

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
        Command::Scratch(args) => scratch::run(args),
        Command::Worktree(args) => worktree::run(args),
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
        Command::PaneCwd { job_id } => list::run_pane_cwd(job_id),
    }
}

/// `sinteractive session …`
fn session(command: SessionCommand) -> Result<i32> {
    match command {
        SessionCommand::Ensure(args) => ensure::run(args),
        SessionCommand::Peek(args) => peek::run(args),
        SessionCommand::Send(args) => send::run(args),
        SessionCommand::Panes(args) => pane::run(args),
        SessionCommand::Exec(args) => exec::run(args),
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

/// The "advanced" section appended to the generated man page: the
/// `SINTERACTIVE_*` environment variables. Explicit flags win over these;
/// these win over the built-in defaults.
const MAN_ADVANCED: &str = r#".SH ADVANCED
Defaults can be changed with environment variables, usually set in a shell
profile\&. An explicit command\-line flag always wins over the environment,
and the environment wins over the built\-in defaults\&. Unset and empty
variables are treated alike\&. Boolean variables accept
\fBon\fR, \fB1\fR, \fBtrue\fR or \fByes\fR to enable and
\fBoff\fR, \fB0\fR, \fBfalse\fR or \fBno\fR to disable\&.
.SS "Session requests"
.TP
\fBSINTERACTIVE_TIME\fR
Default walltime, in Slurm form (\fB24:00:00\fR) or a human form such as
\fB8h\fR\&. Default \fB24:00:00\fR\&.
.TP
\fBSINTERACTIVE_PARTITION\fR
Default partition\&. Default \fBinteractive\fR\&. On Alpine, set this in your
profile\&.
.TP
\fBSINTERACTIVE_QOS\fR
QOS passed as \fB\-\-qos\fR\&. Only added when set\&.
.TP
\fBSINTERACTIVE_CPUS\fR
Default \fB\-\-cpus\-per\-task\fR\&. Default \fB2\fR\&.
.TP
\fBSINTERACTIVE_MEM\fR
Default \fB\-\-mem\fR\&. Default \fB8G\fR\&.
.SS "Appearance"
.TP
\fBSINTERACTIVE_MOUSE\fR
Mouse mode in the session (clicking the status bar)\&. Default \fBon\fR\&.
.TP
\fBSINTERACTIVE_THEME\fR
\fBdark\fR or \fBlight\fR forces a palette; \fBauto\fR (or unset) asks the
terminal\&.
.TP
\fBSINTERACTIVE_COLOR\fR
\fBauto\fR (default), \fBalways\fR or \fBnever\fR\&. \fBNO_COLOR\fR is
honoured\&.
.TP
\fBSINTERACTIVE_MONITOR_SESSIONS\fR
When on, your other sinteractive sessions are listed in the monitor panel
alongside your jobs\&. Default \fBoff\fR\&.
.SS "Walltime warnings and polling"
.TP
\fBSINTERACTIVE_WARN_YELLOW\fR
Seconds remaining at which the status bar turns yellow\&. Default
\fB3600\fR\&.
.TP
\fBSINTERACTIVE_WARN_RED\fR
Seconds remaining at which the status bar turns red\&. Default \fB600\fR\&.
.TP
\fBSINTERACTIVE_GRACE\fR
Seconds before the limit at which the session ends itself\&. Default
\fB10\fR\&.
.TP
\fBSINTERACTIVE_POLL\fR
Seconds between status refreshes\&. Default \fB30\fR, minimum \fB5\fR\&.
.TP
\fBSINTERACTIVE_AGENT_WARN\fR
Seconds remaining below which the Claude Code walltime\-guard hook warns\&.
Default \fB1800\fR\&.
.SS "Storage quota"
.TP
\fBSINTERACTIVE_QUOTA_POLL\fR
Seconds between quota checks\&. Default \fB600\fR, minimum \fB30\fR\&.
.TP
\fBSINTERACTIVE_QUOTA_FILE\fR
Quota file to read\&. Default \fB/cluster/scripts/quota_current\&.txt\fR
(Bodhi)\&.
.TP
\fBSINTERACTIVE_QUOTA_HOSTS\fR
Quota daemon hosts to query\&. Default the Bodhi daemons\&.
.TP
\fBSINTERACTIVE_QUOTA_PORT\fR
Quota daemon port\&. Default \fB9878\fR\&.
.TP
\fBSINTERACTIVE_QUOTA_TIMEOUT\fR
Seconds to wait for the quota daemon\&. Default \fB5\fR\&.
.SS "Paths"
.TP
\fBSINTERACTIVE_CACHE\fR
State directory\&. Default \fB$XDG_CACHE_HOME/sinteractive\fR, else
\fB~/.cache/sinteractive\fR\&.
.TP
\fBSINTERACTIVE_SHARE\fR
Asset root used by \fBclaude install\fR, overriding the installed location\&.
.TP
\fBSINTERACTIVE_WORKTREES\fR
Where the Claude Code worktree hook creates worktrees
(\fI<here>/<repo>/<name>\fR)\&. Unset: \fB/scratch/alpine/$USER/worktrees\fR
where that exists, else \fB<repo>/\&.claude/worktrees\fR\&.
.SS "Set inside a session"
.TP
\fBSINTERACTIVE_JOB_ID\fR, \fBSINTERACTIVE_NAME\fR
The Slurm job id and session name\&. Set by sinteractive; do not set them
yourself\&.
"#;

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
            let mut buf: Vec<u8> = Vec::new();
            clap_mangen::Man::new(cmd).render(&mut buf)?;
            buf.extend_from_slice(MAN_ADVANCED.as_bytes());
            std::io::stdout().write_all(&buf)?;
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
