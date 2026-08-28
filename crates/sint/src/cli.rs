//! Command-line surface.
//!
//! Bare `sinteractive [LAUNCH OPTIONS] [SBATCH ARGS…]` launches a session;
//! everything else is a subcommand. The 0.x top-level flags (`--status`,
//! `--list`, …) are accepted as hidden aliases for one release and mapped to
//! subcommands by [`Cli::resolve`], which warns once on stderr.
//!
//! sbatch passthrough: like 0.x, any launch-time argument we do not recognise
//! is forwarded to `sbatch` verbatim, in any order. clap cannot express
//! "unknown flags go elsewhere", so [`split_launch_argv`] pre-scans argv
//! using the launch flag table and hands clap only our own arguments.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "sinteractive",
    version,
    about = "Persistent interactive sessions on Slurm compute nodes",
    long_about = None,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Launch options (bare invocation).
    #[command(flatten)]
    pub launch: LaunchArgs,

    /// Hidden 0.x compatibility flags.
    #[command(flatten)]
    pub compat: CompatFlags,
}

#[derive(Args, Debug, Default, Clone)]
pub struct LaunchArgs {
    /// Request a specific compute node (`--nodelist`)
    #[arg(long, value_name = "NODE")]
    pub node: Option<String>,
    /// Slurm partition
    #[arg(short = 'p', long, value_name = "PART")]
    pub partition: Option<String>,
    /// Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`)
    #[arg(short = 't', long, value_name = "TIME")]
    pub time: Option<String>,
    /// Number of CPUs (`--cpus-per-task`)
    #[arg(short = 'j', long = "threads", value_name = "N")]
    pub threads: Option<u32>,
    /// Memory (`--mem`)
    #[arg(short = 'm', long, value_name = "SIZE")]
    pub mem: Option<String>,
    /// Tag the session with a name for easy reattach
    #[arg(short = 'n', long, value_name = "NAME")]
    pub name: Option<String>,
    /// Enable mouse support in the session
    #[arg(long, overrides_with = "no_mouse")]
    pub mouse: bool,
    /// Disable mouse support (overrides `SINTERACTIVE_MOUSE`)
    #[arg(long = "no-mouse")]
    pub no_mouse: bool,
    /// Launch without attaching; print connection info and return
    #[arg(long)]
    pub detach: bool,
    /// Machine-readable JSON output (with `--detach`)
    #[arg(long)]
    pub json: bool,
    /// Additional arguments passed straight to `sbatch` (filled by
    /// [`split_launch_argv`], not by clap).
    #[arg(skip)]
    pub sbatch_args: Vec<String>,
}

/// 0.x flags. All hidden; see [`Cli::resolve`].
#[derive(Args, Debug, Default, Clone)]
pub struct CompatFlags {
    #[arg(long = "status", value_name = "TARGET", num_args = 0..=1, hide = true, default_missing_value = "")]
    pub compat_status: Option<String>,
    #[arg(long = "refresh", value_name = "TARGET", num_args = 0..=1, hide = true, default_missing_value = "")]
    pub compat_refresh: Option<String>,
    #[arg(short = 'l', long = "list", hide = true)]
    pub compat_list: bool,
    #[arg(long = "ensure", value_name = "NAME", hide = true)]
    pub compat_ensure: Option<String>,
    #[arg(short = 'a', long = "attach", value_name = "TARGET", num_args = 0..=1, hide = true, default_missing_value = "")]
    pub compat_attach: Option<String>,
    #[arg(long = "cancel", value_name = "TARGET", hide = true)]
    pub compat_cancel: Option<String>,
    #[arg(long = "check-quota", hide = true)]
    pub compat_check_quota: bool,
    #[arg(long = "agent-context", hide = true)]
    pub compat_agent_context: bool,
    #[arg(long = "install-claude", hide = true)]
    pub compat_install_claude: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Launch a new session (the default when no subcommand is given)
    Launch(LaunchArgs),
    /// Reattach to a session by JOBID or NAME (your only session when omitted)
    Attach(AttachArgs),
    /// Reuse the session named NAME, or launch it if absent (implies --detach)
    Ensure(EnsureArgs),
    /// Show one session's status
    Status(TargetArgs),
    /// Re-check a session's time budget now and update its cache
    Refresh(TargetArgs),
    /// List running sessions
    List(JsonFlag),
    /// Cancel a session
    Cancel(CancelArgs),
    /// Your job queue: running, pending (with reasons), and recent history
    Queue(QueueArgs),
    /// Live CPU/GPU/process view of a session's node, or any host
    Monitor(MonitorArgs),
    /// One-shot resource sample of this host
    Snapshot(JsonFlag),
    /// Stream session events (NDJSON)
    Events(EventsArgs),
    /// Read the last lines of a session's screen
    Peek(PeekArgs),
    /// Type a command into a session's shell
    Send(SendArgs),
    /// Brief a coding agent on the session it is running inside
    AgentContext,
    /// Storage quota (Bodhi daemons; wraps curc-quota on Alpine)
    Quota(QuotaArgs),
    /// Claude Code hook entry points
    Hook(HookArgs),
    /// Claude Code statusLine command
    Statusline,
    /// MCP server over stdio
    Mcp,
    /// Install the Claude Code skills, hooks, statusline and MCP server
    InstallClaude,
    /// Check this install and, optionally, every compute node
    Doctor(DoctorArgs),
    /// Print shell completions
    Completions { shell: clap_complete::Shell },
    /// Print the man page (roff)
    Man,
    /// Dump the JSON schemas of the machine-readable outputs
    Schema,

    // ---- internal verbs (hidden) --------------------------------------
    /// The batch job body: starts zellij on the node and babysits it
    #[command(name = "__job", hide = true)]
    Job(JobArgs),
    /// Runs on the node over ssh: attach the local zellij client
    #[command(name = "__attach", hide = true)]
    AttachLocal { session: String },
    /// In-session floating views
    #[command(name = "__popup", hide = true)]
    Popup { view: PopupView, job_id: u64 },
}

#[derive(Args, Debug, Clone, Default)]
pub struct JsonFlag {
    /// Machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct TargetArgs {
    /// JOBID or NAME; defaults to the current session
    pub target: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct AttachArgs {
    /// JOBID or NAME; with no target, your only session
    pub target: Option<String>,
    /// Attach over ssh -X (X11 forwarding) instead of srun --overlap
    #[arg(long)]
    pub ssh: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct EnsureArgs {
    pub name: String,
    #[command(flatten)]
    pub launch: LaunchArgs,
}

#[derive(Args, Debug, Clone, Default)]
pub struct CancelArgs {
    pub target: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct QueueArgs {
    /// Include every user's jobs in the partitions you can see
    #[arg(long)]
    pub all: bool,
    /// Refresh continuously
    #[arg(long)]
    pub watch: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct MonitorArgs {
    /// JOBID, NAME, or a hostname; defaults to the current session
    pub target: Option<String>,
    /// Sample over ssh at 1 Hz instead of reading the shared snapshot
    #[arg(long)]
    pub live: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct EventsArgs {
    pub target: Option<String>,
    #[arg(long)]
    pub follow: bool,
    /// Only events after this epoch
    #[arg(long)]
    pub since: Option<i64>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct PeekArgs {
    pub target: String,
    #[arg(short = 'n', long, default_value_t = 100)]
    pub lines: usize,
}

#[derive(Args, Debug, Clone, Default)]
pub struct SendArgs {
    pub target: String,
    pub command: String,
}

#[derive(Args, Debug, Clone, Default)]
pub struct QuotaArgs {
    /// Probe now instead of reading the cache, and poke every running session
    #[arg(long)]
    pub check: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct HookArgs {
    #[command(subcommand)]
    pub event: HookEvent,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HookEvent {
    /// SessionStart: print the agent briefing
    SessionStart,
    /// UserPromptSubmit: warn when walltime is short
    Prompt,
}

#[derive(Args, Debug, Clone, Default)]
pub struct DoctorArgs {
    /// Sweep every node from sinfo over ssh
    #[arg(long)]
    pub nodes: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct JobArgs {
    #[arg(long)]
    pub mouse: bool,
    #[arg(long = "session-name")]
    pub session_name: Option<String>,
    /// `NAME@EPOCH` — maintenance reservation the request was trimmed to fit
    #[arg(long)]
    pub maint: Option<String>,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupView {
    Monitor,
    Queue,
    Help,
    Notices,
}

impl Cli {
    /// Map 0.x compatibility flags onto a [`Command`]. Returns the command
    /// and whether a deprecation warning is due.
    pub fn resolve(self) -> (Command, bool) {
        let c = &self.compat;
        let json = self.launch.json;
        let target = |s: &Option<String>| s.clone().filter(|t| !t.is_empty());
        if let Some(cmd) = self.command {
            return (cmd, false);
        }
        let cmd = if c.compat_list {
            Command::List(JsonFlag { json })
        } else if let Some(t) = &c.compat_status {
            Command::Status(TargetArgs {
                target: target(&Some(t.clone())),
                json,
            })
        } else if let Some(t) = &c.compat_refresh {
            Command::Refresh(TargetArgs {
                target: target(&Some(t.clone())),
                json,
            })
        } else if let Some(name) = &c.compat_ensure {
            let mut launch = self.launch.clone();
            launch.detach = true;
            Command::Ensure(EnsureArgs {
                name: name.clone(),
                launch,
            })
        } else if let Some(t) = &c.compat_attach {
            Command::Attach(AttachArgs {
                target: target(&Some(t.clone())),
                ssh: false,
            })
        } else if let Some(t) = &c.compat_cancel {
            Command::Cancel(CancelArgs {
                target: t.clone(),
                json,
            })
        } else if c.compat_check_quota {
            Command::Quota(QuotaArgs { check: true, json })
        } else if c.compat_agent_context {
            Command::AgentContext
        } else if c.compat_install_claude {
            Command::InstallClaude
        } else {
            return (Command::Launch(self.launch), false);
        };
        (cmd, true)
    }
}

/// Split raw argv (after the program name) into the arguments clap should
/// see and the ones to forward to `sbatch`. Only applies to a bare launch or
/// `launch`/`ensure` invocations; for any other subcommand `sbatch` is empty
/// and `ours` is argv unchanged.
///
/// Recognised launch flags (with value): `--node --partition -p --time -t
/// --threads -j --mem -m --name -n`; (boolean): `--mouse --no-mouse --detach
/// --json`; plus every compat flag; `-h/--help/-V/--version`. Both `--flag
/// VALUE` and `--flag=VALUE` are ours. A bundled short flag like `-la` or
/// `-nfoo` is rejected with a "did you mean" hint (script lines 438-454)
/// rather than forwarded. `--` ends our parsing; the rest is sbatch's.
pub fn split_launch_argv(_argv: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    // TODO(phase-1/agent-C)
    unimplemented!()
}
