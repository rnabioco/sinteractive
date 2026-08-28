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
    /// The embedded zellij's own command line (`sinteractive zellij --help`)
    #[command(external_subcommand)]
    Zellij(Vec<String>),

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
    /// Session name (the positional; distinct clap id from `--name`, which
    /// the flattened launch flags also carry).
    #[arg(id = "ensure_name", value_name = "NAME")]
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
pub fn split_launch_argv(argv: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let mut ours: Vec<String> = Vec::new();
    let mut sbatch: Vec<String> = Vec::new();
    let Some(first) = argv.first() else {
        return Ok((ours, sbatch));
    };

    // Help/version and every subcommand except launch/ensure: clap owns the
    // whole line, nothing is sbatch's.
    let mut i = 0;
    let mut ensure_name_pending = false;
    match first.as_str() {
        "-h" | "--help" | "-V" | "--version" => return Ok((argv.to_vec(), sbatch)),
        "launch" => {
            ours.push(first.clone());
            i = 1;
        }
        "ensure" => {
            ours.push(first.clone());
            ensure_name_pending = true;
            i = 1;
        }
        s if is_subcommand(s) => return Ok((argv.to_vec(), sbatch)),
        _ => {}
    }

    while i < argv.len() {
        let tok = argv[i].as_str();
        i += 1;

        if tok == "--" {
            sbatch.extend(argv[i..].iter().cloned());
            break;
        }

        // --flag=value forms of our value-taking flags.
        if let Some((flag, _value)) = tok.split_once('=') {
            if tok.starts_with("--")
                && (VALUE_LONG.contains(&flag) || OPTIONAL_LONG.contains(&flag))
            {
                ours.push(tok.to_string());
                continue;
            }
        }

        if BOOL_FLAGS.contains(&tok) {
            ours.push(tok.to_string());
            continue;
        }

        if VALUE_LONG.contains(&tok) || VALUE_SHORT.contains(&tok) {
            // bash: `-n --foo` takes "--foo" as the name; only a missing
            // token is an error. --ensure/--cancel additionally reject a
            // following flag (script lines 292-297, 336-344).
            let strict = matches!(tok, "--ensure" | "--cancel");
            match argv.get(i) {
                Some(v) if !(strict && v.starts_with('-')) => {
                    ours.push(tok.to_string());
                    ours.push(v.clone());
                    i += 1;
                }
                _ => return Err(format!("{tok} requires a {} argument.", value_noun(tok))),
            }
            continue;
        }

        if OPTIONAL_LONG.contains(&tok) || tok == "-a" {
            ours.push(tok.to_string());
            if let Some(v) = argv.get(i) {
                if !v.starts_with('-') {
                    ours.push(v.clone());
                    i += 1;
                }
            }
            continue;
        }

        // Bundled short flags / attached values (script lines 438-454).
        if let Some(rest) = tok.strip_prefix('-') {
            if !rest.starts_with('-') && rest.len() >= 2 {
                let mut chars = rest.chars();
                let flag = chars.next().unwrap();
                let second = chars.next().unwrap();
                if "ahjlmnt".contains(flag) && second.is_ascii_alphabetic() {
                    return Err(bundled_short_error(tok, flag));
                }
                if "ptjmna".contains(flag) && !second.is_ascii_alphabetic() {
                    // `-t8h`, `-j4`, `-m16G`: clap accepts the attached form.
                    ours.push(tok.to_string());
                    continue;
                }
            }
        }

        // Unknown flag: sbatch's, together with a following bare value.
        if tok.starts_with('-') {
            sbatch.push(tok.to_string());
            if !tok.contains('=') {
                if let Some(v) = argv.get(i) {
                    if !v.starts_with('-') {
                        sbatch.push(v.clone());
                        i += 1;
                    }
                }
            }
            continue;
        }

        // Bare token: the ensure NAME once, otherwise sbatch's.
        if ensure_name_pending {
            ours.push(tok.to_string());
            ensure_name_pending = false;
        } else {
            sbatch.push(tok.to_string());
        }
    }

    Ok((ours, sbatch))
}

/// Launch flags that take a value (long form).
const VALUE_LONG: &[&str] = &[
    "--node",
    "--partition",
    "--time",
    "--threads",
    "--mem",
    "--name",
    "--ensure",
    "--cancel",
];
/// Launch flags that take a value (short form).
const VALUE_SHORT: &[&str] = &["-p", "-t", "-j", "-m", "-n"];
/// Compat flags whose target is optional: the next token is theirs only when
/// it does not start with `-`.
const OPTIONAL_LONG: &[&str] = &["--status", "--refresh", "--attach"];
/// Our boolean flags.
const BOOL_FLAGS: &[&str] = &[
    "--mouse",
    "--no-mouse",
    "--detach",
    "--json",
    "--list",
    "-l",
    "--check-quota",
    "--agent-context",
    "--install-claude",
    "-h",
    "--help",
    "-V",
    "--version",
];

fn value_noun(flag: &str) -> &'static str {
    match flag {
        "--node" => "NODE",
        "--partition" | "-p" => "PARTITION",
        "--time" | "-t" => "TIME",
        "--threads" | "-j" => "CPU-count",
        "--mem" | "-m" => "SIZE",
        "--name" | "-n" | "--ensure" => "NAME",
        "--cancel" => "JOBID or NAME",
        _ => "value",
    }
}

fn bundled_short_error(tok: &str, flag: char) -> String {
    let hint = match flag {
        'a' => "-a NAME (--attach)",
        'h' => "-h (--help)",
        'j' => "-j N (--threads)",
        'l' => "-l (--list)",
        'm' => "-m SIZE",
        'n' => "-n NAME (--name)",
        _ => "-t TIME",
    };
    format!(
        "unrecognized option '{tok}'.\n\
         Short options can't be combined or take attached values — \
         did you mean '{hint}'?\n\
         Run 'sinteractive --help' to see all options."
    )
}

/// Whether `s` names a subcommand (hidden ones included) or clap's `help`.
fn is_subcommand(s: &str) -> bool {
    use clap::CommandFactory;
    s == "help" || Cli::command().get_subcommands().any(|c| c.get_name() == s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    fn split(args: &[&str]) -> (Vec<String>, Vec<String>) {
        split_launch_argv(&v(args)).expect("split ok")
    }

    /// Parse `ours` the way `main` does and return the resolved command.
    fn parse(args: &[&str]) -> (Command, Vec<String>, bool) {
        let (ours, sbatch) = split(args);
        let mut full = vec!["sinteractive".to_string()];
        full.extend(ours);
        let cli = Cli::try_parse_from(full).expect("clap accepts the split");
        let (cmd, deprecated) = cli.resolve();
        (cmd, sbatch, deprecated)
    }

    fn launch(args: &[&str]) -> (LaunchArgs, Vec<String>) {
        match parse(args) {
            (Command::Launch(l), sbatch, false) => (l, sbatch),
            other => panic!("expected an undeprecated launch, got {other:?}"),
        }
    }

    #[test]
    fn empty_argv_is_a_bare_launch() {
        assert_eq!(split(&[]), (vec![], vec![]));
        let (_, sbatch) = launch(&[]);
        assert!(sbatch.is_empty());
    }

    #[test]
    fn name_and_time_are_ours_gres_is_sbatchs() {
        let args = ["-n", "foo", "--gres=gpu:1", "-t", "8h"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&["-n", "foo", "-t", "8h"]));
        assert_eq!(sbatch, v(&["--gres=gpu:1"]));
        let (l, sbatch) = launch(&args);
        assert_eq!(l.name.as_deref(), Some("foo"));
        assert_eq!(l.time.as_deref(), Some("8h"));
        assert_eq!(sbatch, v(&["--gres=gpu:1"]));
    }

    #[test]
    fn unknown_flag_with_separate_value_forwards_both() {
        let args = ["--gres", "gpu:1", "-j", "4"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&["-j", "4"]));
        assert_eq!(sbatch, v(&["--gres", "gpu:1"]));
        let (l, _) = launch(&args);
        assert_eq!(l.threads, Some(4));
    }

    #[test]
    fn unknown_flags_keep_their_order() {
        let (ours, sbatch) = split(&[
            "--exclusive",
            "-n",
            "x",
            "--qos=long",
            "--nodelist",
            "n01",
            "--mouse",
            "-w",
            "n02",
            "script-arg",
        ]);
        assert_eq!(ours, v(&["-n", "x", "--mouse"]));
        assert_eq!(
            sbatch,
            v(&[
                "--exclusive",
                "--qos=long",
                "--nodelist",
                "n01",
                "-w",
                "n02",
                "script-arg"
            ])
        );
    }

    #[test]
    fn bundled_short_flags_are_rejected_with_a_hint() {
        let err = split_launch_argv(&v(&["-la"])).unwrap_err();
        assert!(err.contains("unrecognized option '-la'"), "{err}");
        assert!(err.contains("did you mean '-l (--list)'"), "{err}");
        for (tok, hint) in [
            ("-nfoo", "-n NAME (--name)"),
            ("-hx", "-h (--help)"),
            ("-ab", "-a NAME (--attach)"),
            ("-jx", "-j N (--threads)"),
            ("-mx", "-m SIZE"),
            ("-tx", "-t TIME"),
            ("-nf00", "-n NAME (--name)"),
        ] {
            let err = split_launch_argv(&v(&[tok])).unwrap_err();
            assert!(err.contains(hint), "{tok}: {err}");
        }
    }

    #[test]
    fn attached_non_letter_values_are_ours() {
        let args = ["-t8h", "-j4", "-m16G", "-n1"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&args));
        assert!(sbatch.is_empty());
        let (l, _) = launch(&args);
        assert_eq!(l.time.as_deref(), Some("8h"));
        assert_eq!(l.threads, Some(4));
        assert_eq!(l.mem.as_deref(), Some("16G"));
        assert_eq!(l.name.as_deref(), Some("1"));
    }

    #[test]
    fn other_attached_short_forms_pass_through_like_bash() {
        // -w is not ours; -pXX with letters is what bash forwarded too (it
        // had no -p of its own) and sbatch reads it as --partition; -l5 is
        // outside the typo guard and was forwarded verbatim.
        let (ours, sbatch) = split(&["-wnode01", "-pamilan", "-l5"]);
        assert!(ours.is_empty());
        assert_eq!(sbatch, v(&["-wnode01", "-pamilan", "-l5"]));
    }

    #[test]
    fn subcommands_are_untouched() {
        let (ours, sbatch) = split(&["status", "--json"]);
        assert_eq!(ours, v(&["status", "--json"]));
        assert!(sbatch.is_empty());
        let (ours, sbatch) = split(&["cancel", "--gres=gpu:1", "12345"]);
        assert_eq!(ours, v(&["cancel", "--gres=gpu:1", "12345"]));
        assert!(sbatch.is_empty());
        for sub in ["__job", "__attach", "__popup", "help", "completions"] {
            let (ours, sbatch) = split(&[sub, "-n", "x"]);
            assert_eq!(ours, v(&[sub, "-n", "x"]));
            assert!(sbatch.is_empty());
        }
        let (cmd, _, dep) = parse(&["status", "--json"]);
        assert!(matches!(
            cmd,
            Command::Status(TargetArgs { json: true, .. })
        ));
        assert!(!dep);
    }

    #[test]
    fn help_and_version_are_untouched() {
        for a in ["-h", "--help", "-V", "--version"] {
            let (ours, sbatch) = split(&[a, "--gres=gpu:1"]);
            assert_eq!(ours, v(&[a, "--gres=gpu:1"]));
            assert!(sbatch.is_empty());
        }
    }

    #[test]
    fn compat_status_with_target_and_json_is_ours() {
        let args = ["--status", "147845", "--json"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&args));
        assert!(sbatch.is_empty());
        let (cmd, _, dep) = parse(&args);
        match cmd {
            Command::Status(t) => {
                assert_eq!(t.target.as_deref(), Some("147845"));
                assert!(t.json);
            }
            other => panic!("expected status, got {other:?}"),
        }
        assert!(dep);
    }

    #[test]
    fn compat_optional_targets_do_not_eat_flags() {
        let (ours, sbatch) = split(&["--status", "--json"]);
        assert_eq!(ours, v(&["--status", "--json"]));
        assert!(sbatch.is_empty());
        match parse(&["--status", "--json"]).0 {
            Command::Status(t) => {
                assert_eq!(t.target, None);
                assert!(t.json);
            }
            other => panic!("expected status, got {other:?}"),
        }
        assert!(matches!(
            parse(&["-a"]).0,
            Command::Attach(AttachArgs { target: None, .. })
        ));
        match parse(&["--attach=web"]).0 {
            Command::Attach(a) => assert_eq!(a.target.as_deref(), Some("web")),
            other => panic!("expected attach, got {other:?}"),
        }
        match parse(&["-a", "web"]).0 {
            Command::Attach(a) => assert_eq!(a.target.as_deref(), Some("web")),
            other => panic!("expected attach, got {other:?}"),
        }
        match parse(&["--refresh", "web"]).0 {
            Command::Refresh(t) => assert_eq!(t.target.as_deref(), Some("web")),
            other => panic!("expected refresh, got {other:?}"),
        }
    }

    #[test]
    fn compat_list_and_flags() {
        let (cmd, _, dep) = parse(&["-l", "--json"]);
        assert!(matches!(cmd, Command::List(JsonFlag { json: true })));
        assert!(dep);
        assert!(matches!(
            parse(&["--list"]).0,
            Command::List(JsonFlag { json: false })
        ));
        assert!(matches!(
            parse(&["--cancel", "web"]).0,
            Command::Cancel(CancelArgs { .. })
        ));
        assert!(matches!(
            parse(&["--cancel=12"]).0,
            Command::Cancel(CancelArgs { .. })
        ));
        assert!(matches!(
            parse(&["--check-quota"]).0,
            Command::Quota(QuotaArgs { check: true, .. })
        ));
        assert!(matches!(
            parse(&["--agent-context"]).0,
            Command::AgentContext
        ));
        assert!(matches!(
            parse(&["--install-claude"]).0,
            Command::InstallClaude
        ));
    }

    #[test]
    fn compat_ensure_keeps_launch_flags_and_forwards_the_rest() {
        let args = ["--ensure", "web", "-j", "4", "--gres=gpu:1"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&["--ensure", "web", "-j", "4"]));
        assert_eq!(sbatch, v(&["--gres=gpu:1"]));
        let (cmd, sbatch, dep) = parse(&args);
        match cmd {
            Command::Ensure(e) => {
                assert_eq!(e.name, "web");
                assert_eq!(e.launch.threads, Some(4));
                assert!(e.launch.detach);
            }
            other => panic!("expected ensure, got {other:?}"),
        }
        assert_eq!(sbatch, v(&["--gres=gpu:1"]));
        assert!(dep);
        let (ours, _) = split(&["--ensure=web"]);
        assert_eq!(ours, v(&["--ensure=web"]));
    }

    #[test]
    fn ensure_subcommand_keeps_its_name() {
        let args = ["ensure", "web", "--gres=gpu:1", "-t", "2h", "--exclusive"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&["ensure", "web", "-t", "2h"]));
        assert_eq!(sbatch, v(&["--gres=gpu:1", "--exclusive"]));
        let (cmd, _, dep) = parse(&args);
        match cmd {
            Command::Ensure(e) => {
                assert_eq!(e.name, "web");
                assert_eq!(e.launch.time.as_deref(), Some("2h"));
            }
            other => panic!("expected ensure, got {other:?}"),
        }
        assert!(!dep);
        // Flags before the name are fine too; later bare tokens are sbatch's.
        let (ours, sbatch) = split(&["ensure", "-j", "2", "web", "extra"]);
        assert_eq!(ours, v(&["ensure", "-j", "2", "web"]));
        assert_eq!(sbatch, v(&["extra"]));
    }

    #[test]
    fn launch_subcommand_is_split_like_a_bare_launch() {
        let args = ["launch", "--detach", "--json", "--gres", "gpu:1"];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&["launch", "--detach", "--json"]));
        assert_eq!(sbatch, v(&["--gres", "gpu:1"]));
        let (l, sbatch) = launch(&args);
        assert!(l.detach);
        assert!(l.json);
        assert_eq!(sbatch, v(&["--gres", "gpu:1"]));
    }

    #[test]
    fn double_dash_ends_our_parsing() {
        let (ours, sbatch) = split(&["--", "-n", "x"]);
        assert!(ours.is_empty());
        assert_eq!(sbatch, v(&["-n", "x"]));
        let (ours, sbatch) = split(&["-n", "a", "--", "--json", "-t", "1h"]);
        assert_eq!(ours, v(&["-n", "a"]));
        assert_eq!(sbatch, v(&["--json", "-t", "1h"]));
    }

    #[test]
    fn long_equals_forms_are_ours() {
        let args = [
            "--name=x",
            "--time=1h",
            "--threads=2",
            "--mem=4G",
            "--node=n01",
            "--partition=p",
        ];
        let (ours, sbatch) = split(&args);
        assert_eq!(ours, v(&args));
        assert!(sbatch.is_empty());
        let (l, _) = launch(&args);
        assert_eq!(l.name.as_deref(), Some("x"));
        assert_eq!(l.time.as_deref(), Some("1h"));
        assert_eq!(l.threads, Some(2));
        assert_eq!(l.mem.as_deref(), Some("4G"));
        assert_eq!(l.node.as_deref(), Some("n01"));
        assert_eq!(l.partition.as_deref(), Some("p"));
        // A boolean given a value is not ours (bash forwarded it too).
        let (ours, sbatch) = split(&["--json=1"]);
        assert!(ours.is_empty());
        assert_eq!(sbatch, v(&["--json=1"]));
    }

    #[test]
    fn value_flags_take_the_next_token_even_when_it_looks_like_a_flag() {
        let (ours, _) = split(&["-n", "--weird"]);
        assert_eq!(ours, v(&["-n", "--weird"]));
        let (ours, _) = split(&["--time", "-1"]);
        assert_eq!(ours, v(&["--time", "-1"]));
    }

    #[test]
    fn missing_values_are_errors() {
        for flag in [
            "-n",
            "--name",
            "-t",
            "--time",
            "-j",
            "--threads",
            "-m",
            "--mem",
            "--node",
            "--partition",
            "-p",
        ] {
            let err = split_launch_argv(&v(&[flag])).unwrap_err();
            assert!(err.contains("requires"), "{flag}: {err}");
        }
        let err = split_launch_argv(&v(&["--ensure"])).unwrap_err();
        assert!(err.contains("NAME"), "{err}");
        let err = split_launch_argv(&v(&["--ensure", "--json"])).unwrap_err();
        assert!(err.contains("NAME"), "{err}");
        let err = split_launch_argv(&v(&["--cancel", "-l"])).unwrap_err();
        assert!(err.contains("JOBID or NAME"), "{err}");
    }

    #[test]
    fn mouse_flags_are_ours() {
        let (l, _) = launch(&["--no-mouse", "--mouse"]);
        assert!(l.mouse);
        let (l, _) = launch(&["--mouse", "--no-mouse"]);
        assert!(!l.mouse);
        assert!(l.no_mouse);
    }

    #[test]
    fn clap_surface_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
