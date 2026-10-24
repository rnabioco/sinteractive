//! `sinteractive mcp` — the Model Context Protocol server, over stdio.
//!
//! Every tool is a thin wrapper over the same data functions the CLI's
//! `--json` paths print (`list_data`, `ensure_data`, `queue_data`, …), and
//! returns that function's serde struct as structured content, so the tool
//! schemas and the documented JSON contracts cannot drift apart. Failures a
//! caller should see — a name that matches nothing, a session that is not
//! running, no snapshot yet — come back as error results (`isError: true`)
//! with the CLI's wording; only a broken request is a protocol error.
//!
//! stdout is the protocol stream and nothing else may write to it. The
//! launch path narrates on stderr, which is the server's log; stdout stays
//! clean because [`bring_up`](super::launch::bring_up) never prints there.
//!
//! Slurm, ssh and the event tail block, so each tool runs its work on
//! tokio's blocking pool and the protocol loop stays responsive to a
//! cancellation or a second call meanwhile.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use rmcp::handler::server::tool::{IntoCallToolResult, ToolRouter};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{
    CallToolResponse, CallToolResult, ContentBlock, ErrorData, Implementation, ServerCapabilities,
    ServerConfig,
};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sint_core::metrics::Snapshot;
use sint_core::notices::Notice;
use sint_core::now_epoch;
use sint_core::quota::QuotaSnapshot;
use sint_core::session::SessionInfo;
use sint_core::state::StateDir;
use sint_core::usage::UsageReport;

use super::agent_context::{context, AgentContext};
use super::cancel::{cancel_job, CancelResult};
use super::common::Ctx;
use super::ensure::{ensure_data, Ensured};
use super::exec::{run_in_session, ExecResult, TIMEOUT_DEFAULT_SECS, TIMEOUT_MAX_SECS};
use super::jobwait::{self, JobLog, WaitedJobs};
use super::list::{list_data, ListRow};
use super::pane::{list_panes, PaneInfo};
use super::peek::{dump_screen, tail_lines};
use super::queue::{queue_data, QueueReport};
use super::quota::{quota_data, top_data};
use super::scratch::{new_data, ScratchDir};
use super::send::send_command;
use crate::cli::LaunchArgs;

/// What the client is told at `initialize`.
const INSTRUCTIONS: &str =
    "sinteractive keeps persistent shells (zellij sessions) on Slurm compute \
nodes. A session is an orchestration shell, not a compute target: it is a small allocation shared \
with the shell the user is typing in, so editing, git and scheduler queries belong there and \
anything heavier gets its own srun/salloc allocation. Call session_status before long work to \
read the remaining walltime, and wait_for_event (which blocks until something happens) instead \
of polling. Waiting and status checks are trivial work: run them from a subagent on a cheaper \
model (the job-watch skill, or an Agent call with model haiku) rather than the main \
conversation, which replays everything said so far on every wake-up. peek reads a session's \
screen (any pane; list_panes names them); run_in_session runs a short command in a new pane and \
returns its output, which is the way to act in a session; send types into the user's live \
shell, so only do that when asked. wait_for_jobs blocks until Slurm jobs finish and reports how \
they ended; job_log reads a job's stdout.";

/// Default and cap for `wait_for_event`'s timeout, in seconds.
const WAIT_DEFAULT_SECS: u64 = 300;
const WAIT_MAX_SECS: u64 = 3600;
/// How often the event tail looks again.
const WAIT_POLL: Duration = Duration::from_secs(1);

pub fn run() -> Result<i32> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let service = SintServer::new()
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| anyhow!("mcp: {e}"))?;
        service.waiting().await?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(0)
}

/// A tool failure the caller should see: an error result carrying the
/// CLI's wording (or its JSON error object) as text.
#[derive(Debug)]
pub struct ToolError(Box<CallToolResult>);

impl ToolError {
    fn msg(msg: impl std::fmt::Display) -> Self {
        Self::text(msg.to_string())
    }
    /// An error object, as the CLI would print it with `--json`.
    fn json(value: Value) -> Self {
        Self::text(value.to_string())
    }
    fn text(text: String) -> Self {
        ToolError(Box::new(CallToolResult::error(vec![ContentBlock::text(
            text,
        )])))
    }
}

impl From<anyhow::Error> for ToolError {
    fn from(e: anyhow::Error) -> Self {
        ToolError::msg(format!("{e:#}"))
    }
}

impl IntoCallToolResult for ToolError {
    fn into_call_tool_result(self) -> Result<CallToolResponse, ErrorData> {
        Ok((*self.0).into())
    }
}

type ToolResult<T> = Result<T, ToolError>;

// ---- parameters ---------------------------------------------------------

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct TargetParams {
    /// JOBID or session NAME; omitted means the session this server runs
    /// inside (`SINTERACTIVE_JOB_ID`).
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RequiredTargetParams {
    /// JOBID or session NAME.
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EnsureParams {
    /// Session name: letters, digits, `.`, `_`, `-`.
    pub name: String,
    /// Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`); the
    /// configured default when omitted.
    pub time: Option<String>,
    /// CPUs (`--cpus-per-task`).
    pub cpus: Option<u32>,
    /// Memory (`--mem`), e.g. `16G`.
    pub mem: Option<String>,
    /// Slurm partition.
    pub partition: Option<String>,
    /// Extra options passed to sbatch verbatim.
    pub sbatch_args: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct QueueParams {
    /// Also count everyone's jobs per partition.
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PeekParams {
    /// JOBID or session NAME; must be RUNNING.
    pub target: String,
    /// How many lines from the bottom of the screen (default 100).
    pub lines: Option<usize>,
    /// Pane to read: an id from `list_panes` (`terminal_3`), a pane title,
    /// or `focused`. Default `terminal_0`, the session's shell.
    pub pane: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendParams {
    /// JOBID or session NAME; must be RUNNING.
    pub target: String,
    /// Typed into the pane, followed by Enter.
    pub command: String,
    /// Pane to type into: an id from `list_panes` (`terminal_3`), a pane
    /// title, or `focused`. Default `terminal_0`, the session's shell.
    pub pane: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunParams {
    /// JOBID or session NAME; must be RUNNING.
    pub target: String,
    /// Run with `bash -lc` in a new pane.
    pub command: String,
    /// Working directory for the command.
    pub cwd: Option<String>,
    /// Give up after this long and close the pane (default 120, at most
    /// 3600).
    pub timeout_secs: Option<u64>,
    /// Leave the pane open afterwards, for the user to look at.
    pub keep: Option<bool>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WaitJobsParams {
    /// Slurm job ids to wait for (any of the user's jobs, not only
    /// sessions).
    pub job_ids: Option<Vec<u64>>,
    /// Also wait for the user's live jobs with this name; when none are
    /// live, report the last day's finished jobs of that name.
    pub name: Option<String>,
    /// Give up after this long (default 300, at most 3600).
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobLogParams {
    /// Slurm job id.
    pub job_id: u64,
    /// How many lines from the end of the log (default 50).
    pub lines: Option<usize>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct QuotaParams {
    /// Probe the quota daemons now instead of reading the cache.
    pub check: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScratchNewParams {
    /// Letters, digits, `.`, `_`, `-`; names the directory.
    pub topic: String,
    /// Hold it against `scratch clean` for this long (`30d`, `36h`, `2w`).
    pub keep_for: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct QuotaTopParams {
    /// How many directories to list (default 15).
    pub n: Option<usize>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WaitParams {
    /// JOBID or session NAME; omitted means the current session.
    pub target: Option<String>,
    /// Event kinds to wait for (any kind when omitted or empty), e.g.
    /// `walltime_warn`, `walltime_red`, `ended` (`session_ended` is an alias).
    pub kinds: Option<Vec<String>>,
    /// Give up after this long (default 300, at most 3600).
    pub timeout_secs: Option<u64>,
}

// ---- outputs ------------------------------------------------------------

/// `list_sessions`: the `list --json` rows under one key, since a tool's
/// structured content must be an object.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SessionList {
    pub sessions: Vec<ListRow>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PeekResult {
    pub job_id: u64,
    pub node: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PaneList {
    pub job_id: u64,
    pub node: String,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SendResult {
    pub job_id: u64,
    pub sent: bool,
}

/// `session_status`: the `status --json` object, plus the session's active
/// notices (what the status line's "N notices" counter stands for).
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StatusReport {
    #[serde(flatten)]
    pub info: SessionInfo,
    pub notices: Vec<Notice>,
}

/// `monitor_snapshot`: the sampler's snapshot, plus how old it is.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SnapshotReport {
    #[serde(flatten)]
    pub snapshot: Snapshot,
    /// Seconds since the snapshot was taken.
    pub age_secs: u64,
    /// Older than 30 s, the point at which `monitor` calls a snapshot gone.
    pub stale: bool,
}

// ---- the server ---------------------------------------------------------

pub struct SintServer {
    ctx: Arc<Ctx>,
    launch: Option<Launch>,
    tool_router: ToolRouter<Self>,
}

impl SintServer {
    pub fn new() -> Self {
        SintServer {
            ctx: Arc::new(Ctx::new()),
            launch: Launch::capture(),
            tool_router: Self::tool_router(),
        }
    }

    /// Run `f` on the blocking pool with the shared context.
    async fn blocking<T, F>(&self, f: F) -> ToolResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Ctx) -> ToolResult<T> + Send + 'static,
    {
        let ctx = Arc::clone(&self.ctx);
        tokio::task::spawn_blocking(move || f(&ctx))
            .await
            .map_err(|e| ToolError::msg(format!("tool task failed: {e}")))?
    }
}

/// The binary this server started from: the path it was launched by and
/// what that path resolved to then. `sinteractive` upgrades replace the
/// file behind the path, but a running server keeps the old image and its
/// old tool list.
#[derive(Debug, Clone)]
struct Launch {
    path: PathBuf,
    resolved: PathBuf,
}

impl Launch {
    fn capture() -> Option<Self> {
        let path = PathBuf::from(std::env::args_os().next()?);
        if !path.is_absolute() {
            return None;
        }
        let resolved = fs::canonicalize(&path).ok()?;
        Some(Launch { path, resolved })
    }

    /// The path now resolves elsewhere, or nowhere.
    fn is_stale(&self) -> bool {
        fs::canonicalize(&self.path).map_or(true, |now| now != self.resolved)
    }
}

const STALE_NOTE: &str = "NOTE: sinteractive was upgraded after this MCP server started, so its \
tool list may lack tools named below (run_in_session, list_panes, wait_for_jobs, job_log). Use the \
CLI equivalents, or restart Claude Code to pick up the new server.\n\n";

impl Default for SintServer {
    fn default() -> Self {
        Self::new()
    }
}

/// `ctx.resolve` with the tool's error shape.
fn resolve(ctx: &Ctx, target: Option<&str>) -> ToolResult<u64> {
    ctx.resolve(target).map_err(ToolError::from)
}

/// The session's status object, or the `NOT_FOUND` object as an error.
fn session_or_not_found(ctx: &Ctx, job_id: u64) -> ToolResult<SessionInfo> {
    ctx.session_info(job_id)?
        .ok_or_else(|| ToolError::json(SessionInfo::not_found(job_id)))
}

#[tool_router]
impl SintServer {
    #[tool(
        name = "list_sessions",
        annotations(read_only_hint = true),
        description = "The user's RUNNING sinteractive sessions (the `sinteractive list --json` rows)."
    )]
    async fn list_sessions(&self) -> Result<Json<SessionList>, ToolError> {
        let sessions = self.blocking(|ctx| Ok(list_data(ctx)?)).await?;
        Ok(Json(SessionList { sessions }))
    }

    #[tool(
        name = "session_status",
        annotations(read_only_hint = true),
        description = "One session's status, including remaining walltime, and its active notices (`sinteractive status --json`). Errors with {\"job_id\",\"state\":\"NOT_FOUND\"} when Slurm no longer knows the job."
    )]
    async fn session_status(
        &self,
        Parameters(p): Parameters<TargetParams>,
    ) -> Result<Json<StatusReport>, ToolError> {
        let report = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, p.target.as_deref())?;
                let info = session_or_not_found(ctx, job_id)?;
                let notices = sint_core::notices::read(&ctx.state, job_id);
                Ok(StatusReport { info, notices })
            })
            .await?;
        Ok(Json(report))
    }

    #[tool(
        name = "ensure_session",
        annotations(idempotent_hint = true, destructive_hint = false),
        description = "Get-or-create: the RUNNING or PENDING session named NAME, else launch one detached under that name and wait until it is up. `created` says which happened."
    )]
    async fn ensure_session(
        &self,
        Parameters(p): Parameters<EnsureParams>,
    ) -> Result<Json<SessionInfo>, ToolError> {
        let info = self
            .blocking(move |ctx| {
                let largs = LaunchArgs {
                    time: p.time,
                    threads: p.cpus,
                    mem: p.mem,
                    partition: p.partition,
                    sbatch_args: p.sbatch_args.unwrap_or_default(),
                    ..LaunchArgs::default()
                };
                match ensure_data(ctx, &p.name, largs)? {
                    Ensured::Session(info) => Ok(*info),
                    Ensured::NotFound(job_id) => {
                        Err(ToolError::json(SessionInfo::not_found(job_id)))
                    }
                    Ensured::Failed(code) => Err(ToolError::json(json!({
                        "error": "launch failed",
                        "exit_code": code,
                        "detail": "see the server's stderr log",
                    }))),
                }
            })
            .await?;
        Ok(Json(info))
    }

    #[tool(
        name = "cancel_session",
        annotations(destructive_hint = true),
        description = "scancel a session by JOBID or NAME (`sinteractive cancel --json`)."
    )]
    async fn cancel_session(
        &self,
        Parameters(p): Parameters<RequiredTargetParams>,
    ) -> Result<Json<CancelResult>, ToolError> {
        let result = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, Some(&p.target))?;
                let result = cancel_job(ctx, job_id);
                match &result.detail {
                    Some(detail) => Err(ToolError::json(json!({
                        "job_id": job_id,
                        "cancelled": false,
                        "error": format!("could not cancel job {job_id}: {detail}"),
                    }))),
                    None => Ok(result),
                }
            })
            .await?;
        Ok(Json(result))
    }

    #[tool(
        name = "queue",
        annotations(read_only_hint = true),
        description = "The user's job queue: running, pending (with reasons and estimated starts) and the last day's history (`sinteractive queue --json`)."
    )]
    async fn queue(
        &self,
        Parameters(p): Parameters<QueueParams>,
    ) -> Result<Json<QueueReport>, ToolError> {
        let all = p.all.unwrap_or(false);
        let report = self.blocking(move |ctx| Ok(queue_data(ctx, all)?)).await?;
        Ok(Json(report))
    }

    #[tool(
        name = "monitor_snapshot",
        annotations(read_only_hint = true),
        description = "The latest resource snapshot of a session's node (CPU, memory, GPU, processes), as its in-session sampler last wrote it, with `age_secs` and `stale` (older than 30 s: the sampler may have stopped). Errors with `no snapshot yet` until the first sample lands."
    )]
    async fn monitor_snapshot(
        &self,
        Parameters(p): Parameters<TargetParams>,
    ) -> Result<Json<SnapshotReport>, ToolError> {
        let report = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, p.target.as_deref())?;
                snapshot_report(&ctx.state, job_id, now_epoch())
            })
            .await?;
        Ok(Json(report))
    }

    #[tool(
        name = "peek",
        annotations(read_only_hint = true),
        description = "The last lines of a RUNNING session's screen, read over ssh (`sinteractive session peek`). Reads the session's shell unless `pane` names another (see list_panes)."
    )]
    async fn peek(
        &self,
        Parameters(p): Parameters<PeekParams>,
    ) -> Result<Json<PeekResult>, ToolError> {
        let result = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, Some(&p.target))?;
                let session = ctx.require_running(job_id)?;
                let screen = dump_screen(ctx, &session, p.pane.as_deref())?;
                let lines = tail_lines(&screen, p.lines.unwrap_or(100))
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                Ok(PeekResult {
                    job_id,
                    node: session.node,
                    lines,
                })
            })
            .await?;
        Ok(Json(result))
    }

    #[tool(
        name = "send",
        annotations(destructive_hint = true),
        description = "Type a command into a RUNNING session's shell (or another `pane`) and press Enter (`sinteractive session send`). This is the user's live shell: only when asked; run_in_session is the way to run something yourself."
    )]
    async fn send(
        &self,
        Parameters(p): Parameters<SendParams>,
    ) -> Result<Json<SendResult>, ToolError> {
        if p.command.trim().is_empty() {
            return Err(ToolError::msg("nothing to send: command is empty"));
        }
        let result = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, Some(&p.target))?;
                let session = ctx.require_running(job_id)?;
                send_command(ctx, &session, &p.command, p.pane.as_deref())?;
                Ok(SendResult { job_id, sent: true })
            })
            .await?;
        Ok(Json(result))
    }

    #[tool(
        name = "list_panes",
        description = "The terminal panes of a RUNNING session (`sinteractive session panes --json`): id, title, command, cwd, focused, floating, exited and exit status, and tab. Pass an id or title as `pane` to peek or send."
    )]
    async fn list_panes(
        &self,
        Parameters(p): Parameters<RequiredTargetParams>,
    ) -> Result<Json<PaneList>, ToolError> {
        let result = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, Some(&p.target))?;
                let session = ctx.require_running(job_id)?;
                let panes = list_panes(ctx, &session)?;
                Ok(PaneList {
                    job_id,
                    node: session.node,
                    panes,
                })
            })
            .await?;
        Ok(Json(result))
    }

    #[tool(
        name = "run_in_session",
        description = "Run a short command (`bash -lc`) in a new unfocused floating pane of a RUNNING session, wait for it, and return its exit code and output (`sinteractive session exec`). Orchestration-sized work only (git, file edits, scheduler queries): the session is a small allocation shared with the user's shell, not a compute target, so anything heavy goes in its own srun/salloc allocation. A command that outlives `timeout_secs` is killed and reported `timed_out`. The pane is closed afterwards unless `keep`."
    )]
    async fn run_in_session(
        &self,
        Parameters(p): Parameters<RunParams>,
    ) -> Result<Json<ExecResult>, ToolError> {
        if p.command.trim().is_empty() {
            return Err(ToolError::msg("nothing to run: command is empty"));
        }
        let timeout = p
            .timeout_secs
            .unwrap_or(TIMEOUT_DEFAULT_SECS)
            .min(TIMEOUT_MAX_SECS);
        let result = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, Some(&p.target))?;
                let session = ctx.require_running(job_id)?;
                Ok(run_in_session(
                    ctx,
                    &session,
                    &p.command,
                    p.cwd.as_deref(),
                    timeout,
                    p.keep.unwrap_or(false),
                )?)
            })
            .await?;
        Ok(Json(result))
    }

    #[tool(
        name = "wait_for_jobs",
        description = "Block until the given Slurm jobs (any of the user's, found by `job_ids` and/or `name`) have finished, then report each: state, exit_code, elapsed against time_limit, max_rss_mb against req_mem_mb, node and the stdout path. `timed_out` is true when some were still running after `timeout_secs`. Use this instead of polling squeue or running /job-watch."
    )]
    async fn wait_for_jobs(
        &self,
        Parameters(p): Parameters<WaitJobsParams>,
    ) -> Result<Json<WaitedJobs>, ToolError> {
        let timeout = Duration::from_secs(
            p.timeout_secs
                .unwrap_or(WAIT_DEFAULT_SECS)
                .min(WAIT_MAX_SECS),
        );
        let waited = self
            .blocking(move |ctx| {
                Ok(jobwait::wait_for_jobs(
                    &ctx.slurm,
                    &p.job_ids.unwrap_or_default(),
                    p.name.as_deref(),
                    timeout,
                    jobwait::POLL,
                )?)
            })
            .await?;
        Ok(Json(waited))
    }

    #[tool(
        name = "job_log",
        description = "The last `lines` (default 50) of a Slurm job's stdout file, found through scontrol while the job is live and sacct after, and read from the shared filesystem. `exists` is false when the file is not there (yet)."
    )]
    async fn job_log(
        &self,
        Parameters(p): Parameters<JobLogParams>,
    ) -> Result<Json<JobLog>, ToolError> {
        let log = self
            .blocking(move |ctx| {
                Ok(jobwait::job_log(
                    &ctx.slurm,
                    p.job_id,
                    p.lines.unwrap_or(50),
                )?)
            })
            .await?;
        Ok(Json(log))
    }

    #[tool(
        name = "agent_context",
        annotations(read_only_hint = true),
        description = "The briefing for an agent running inside a session: job, node, allocation, remaining walltime and the rules of the road (`sinteractive claude context`). Errors outside a session."
    )]
    async fn agent_context(&self) -> Result<Json<AgentContext>, ToolError> {
        let mut context = self.blocking(|ctx| Ok(context(ctx)?)).await?;
        if self.launch.as_ref().is_some_and(Launch::is_stale) {
            context.stale_server = true;
            context.text.insert_str(0, STALE_NOTE);
        }
        Ok(Json(context))
    }

    #[tool(
        name = "quota",
        annotations(read_only_hint = true),
        description = "Storage quota (`sinteractive quota --json`): the cached probe, or a fresh one with `check`. Errors with {\"error\":\"quota unavailable\"} when there is neither."
    )]
    async fn quota(
        &self,
        Parameters(p): Parameters<QuotaParams>,
    ) -> Result<Json<QuotaSnapshot>, ToolError> {
        let check = p.check.unwrap_or(false);
        let snap = self
            .blocking(move |ctx| {
                quota_data(ctx, check)?
                    .ok_or_else(|| ToolError::json(json!({"error": "quota unavailable"})))
            })
            .await?;
        Ok(Json(snap))
    }

    #[tool(
        name = "quota_top",
        annotations(read_only_hint = true),
        description = "What is using the storage quota (`sinteractive quota --top --json`): the largest directories with size, file count, newest file and kind (worktree, cargo-target, cache, other), and files over 1 GiB that are probable copies of each other. Reads a cached index (`age_secs`, `stale` after 24 h); errors, without scanning, when there is none. Build it with `sinteractive quota --reindex`, which submits a batch job."
    )]
    async fn quota_top(
        &self,
        Parameters(p): Parameters<QuotaTopParams>,
    ) -> Result<Json<UsageReport>, ToolError> {
        let n = p.n.unwrap_or(15);
        let report = self.blocking(move |ctx| Ok(top_data(ctx, n)?)).await?;
        Ok(Json(report))
    }

    #[tool(
        name = "scratch_new",
        annotations(idempotent_hint = true, destructive_hint = false),
        description = "Get-or-create a scratch directory `<scratch>/TOPIC/` with a manifest (owner, branch, session, created), so it can be listed and cleaned by age (`sinteractive scratch new TOPIC`). Use it for working files and throwaway results instead of the repo, a worktree or ~/.cache. Returns its `path`; `created` is false when it already existed."
    )]
    async fn scratch_new(
        &self,
        Parameters(p): Parameters<ScratchNewParams>,
    ) -> Result<Json<ScratchDir>, ToolError> {
        let dir = self
            .blocking(move |ctx| Ok(new_data(ctx, &p.topic, p.keep_for.as_deref())?))
            .await?;
        Ok(Json(dir))
    }

    #[tool(
        name = "wait_for_event",
        annotations(read_only_hint = true),
        description = "Block until the session's next event (new lines of its event log, or walltime crossing the warning thresholds), then return it; {\"timed_out\":true} after `timeout_secs`. Use this instead of polling session_status."
    )]
    async fn wait_for_event(
        &self,
        Parameters(p): Parameters<WaitParams>,
    ) -> ToolResult<CallToolResult> {
        let timeout = Duration::from_secs(
            p.timeout_secs
                .unwrap_or(WAIT_DEFAULT_SECS)
                .min(WAIT_MAX_SECS),
        );
        let kinds = p.kinds.unwrap_or_default();
        let event = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, p.target.as_deref())?;
                let thresholds = Thresholds {
                    warn: ctx.cfg.agent_warn,
                    red: ctx.cfg.warn_red,
                };
                Ok(wait_for_event(
                    &ctx.state, job_id, &kinds, thresholds, timeout, WAIT_POLL,
                ))
            })
            .await?;
        Ok(CallToolResult::structured(event))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SintServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "sinteractive",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }
}

/// `<jobid>.metrics.json` from the cache, parsed. The sampler that writes
/// it lives in the session; before its first tick there is nothing to read.
fn read_snapshot(state: &StateDir, job_id: u64) -> ToolResult<Value> {
    let path = state.metrics_file(job_id);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ToolError::json(json!({
                "job_id": job_id,
                "error": "no snapshot yet",
            })));
        }
        Err(e) => {
            return Err(ToolError::msg(format!(
                "could not read {}: {e}",
                path.display()
            )));
        }
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| ToolError::msg(format!("malformed snapshot {}: {e}", path.display())))?;
    if !value.is_object() {
        return Err(ToolError::msg(format!(
            "malformed snapshot {}: not a JSON object",
            path.display()
        )));
    }
    Ok(value)
}

/// The cached snapshot, typed, with its age against `now`.
fn snapshot_report(state: &StateDir, job_id: u64, now: i64) -> ToolResult<SnapshotReport> {
    let value = read_snapshot(state, job_id)?;
    let snapshot: Snapshot = serde_json::from_value(value).map_err(|e| {
        ToolError::msg(format!(
            "malformed snapshot {}: {e}",
            state.metrics_file(job_id).display()
        ))
    })?;
    Ok(SnapshotReport {
        age_secs: snapshot.age_secs(now),
        stale: snapshot.is_stale(now),
        snapshot,
    })
}

// ---- wait_for_event -----------------------------------------------------

/// Remaining-walltime thresholds for the synthetic events, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub warn: i64,
    pub red: i64,
}

/// The event log's name for a kind: `session_ended` is an alias of `ended`.
fn canonical_kind(kind: &str) -> &str {
    if kind == "session_ended" {
        "ended"
    } else {
        kind
    }
}

/// Block until the next event for `job_id`, or `timeout` passes.
///
/// Real events are new lines appended to `<jobid>.events.ndjson` after
/// this call started (one JSON object per line, `{"ts":…,"kind":…,…}`);
/// the first whose `kind` is in `kinds` (any kind when empty) is returned
/// as-is. While no event log exists — the sampler is not running, or is
/// older than the log — the state file stands in: `remaining_seconds`
/// crossing `thresholds.warn` / `thresholds.red` yields a synthetic
/// `walltime_warn` / `walltime_red`, and the state file vanishing yields
/// `ended`, as the real log writes it. Crossing means a transition since the call started, so
/// a session already under a threshold does not fire it again on every
/// call. On timeout the result is `{"timed_out":true}`.
///
/// The file is polled every `poll`; no inotify, so it works on the shared
/// filesystems the cache lives on.
///
/// "After this call started" is measured in whole lines: the sampler
/// appends a line in more than one write now and then, and a call that
/// begins between two of them must not take the tail of that line for the
/// first new event — it would fail to parse and the event would be lost.
/// So the starting offset is the beginning of any unterminated last line,
/// not the end of the file.
pub fn wait_for_event(
    state: &StateDir,
    job_id: u64,
    kinds: &[String],
    thresholds: Thresholds,
    timeout: Duration,
    poll: Duration,
) -> Value {
    let wanted = |kind: &str| kinds.is_empty() || kinds.iter().any(|k| canonical_kind(k) == kind);
    let log = state.events_file(job_id);
    let deadline = Instant::now() + timeout;
    let mut offset = start_of_unfinished_line(&log);
    let mut walltime = WalltimeWatch::new(state, job_id, thresholds, now_epoch());
    loop {
        if let Some(event) = next_logged_event(&log, &mut offset, &wanted) {
            return event;
        }
        if !log.exists() {
            if let Some(event) = walltime.observe(state, job_id, now_epoch()) {
                if wanted(event.kind) {
                    return event.to_value();
                }
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return json!({"timed_out": true});
        }
        std::thread::sleep(poll.min(deadline - now));
    }
}

/// The first complete line past `offset` whose `kind` is wanted, advancing
/// `offset` over every complete line read. Lines that are not JSON objects
/// are skipped. A log shorter than `offset` was replaced; read it afresh.
/// Where a fresh reader of `log` should start: the file's length when it
/// ends in a newline (or is empty or absent), else the offset of the last
/// line's first byte, so that a line caught half-written is read whole once
/// it finishes. Only the tail is scanned, back to the nearest newline.
fn start_of_unfinished_line(log: &Path) -> u64 {
    let Ok(mut file) = fs::File::open(log) else {
        return 0;
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    const CHUNK: u64 = 4096;
    let mut end = len;
    let mut buf = vec![0u8; CHUNK as usize];
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let n = (end - start) as usize;
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut buf[..n]).is_err() {
            return len;
        }
        if let Some(i) = buf[..n].iter().rposition(|&b| b == b'\n') {
            return start + i as u64 + 1;
        }
        end = start;
    }
    0
}

fn next_logged_event(log: &Path, offset: &mut u64, wanted: &dyn Fn(&str) -> bool) -> Option<Value> {
    let mut file = fs::File::open(log).ok()?;
    let len = file.metadata().ok()?.len();
    if len < *offset {
        *offset = 0;
    }
    if len == *offset {
        return None;
    }
    file.seek(SeekFrom::Start(*offset)).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    let mut consumed = 0usize;
    for line in buf.split_inclusive('\n') {
        if !line.ends_with('\n') {
            break;
        }
        consumed += line.len();
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !value.is_object() {
            continue;
        }
        let kind = value.get("kind").and_then(Value::as_str).unwrap_or("");
        if wanted(kind) {
            *offset += consumed as u64;
            return Some(value);
        }
    }
    *offset += consumed as u64;
    None
}

/// A synthetic event derived from the state file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SyntheticEvent {
    ts: i64,
    kind: &'static str,
    remaining_seconds: Option<i64>,
}

impl SyntheticEvent {
    fn to_value(&self) -> Value {
        json!({
            "ts": self.ts,
            "kind": self.kind,
            "remaining_seconds": self.remaining_seconds,
            "synthetic": true,
        })
    }
}

/// Tracks the aged `remaining_seconds` between polls to spot a threshold
/// crossing, and whether the state file has gone.
struct WalltimeWatch {
    thresholds: Thresholds,
    last: Option<i64>,
    had_state: bool,
}

impl WalltimeWatch {
    fn new(state: &StateDir, job_id: u64, thresholds: Thresholds, now: i64) -> Self {
        let file = state.read_state(job_id);
        WalltimeWatch {
            thresholds,
            last: file.as_ref().and_then(|s| s.aged_remaining(now)),
            had_state: file.is_some(),
        }
    }

    fn observe(&mut self, state: &StateDir, job_id: u64, now: i64) -> Option<SyntheticEvent> {
        let Some(file) = state.read_state(job_id) else {
            if self.had_state {
                self.had_state = false;
                return Some(SyntheticEvent {
                    ts: now,
                    kind: "ended",
                    remaining_seconds: None,
                });
            }
            return None;
        };
        self.had_state = true;
        let remaining = file.aged_remaining(now)?;
        let last = self.last.replace(remaining);
        let crossed = |threshold: i64| match last {
            Some(prev) => prev > threshold && remaining <= threshold,
            None => false,
        };
        // Red outranks warn when a single poll skips both.
        let kind = if crossed(self.thresholds.red) {
            "walltime_red"
        } else if crossed(self.thresholds.warn) {
            "walltime_warn"
        } else {
            return None;
        };
        Some(SyntheticEvent {
            ts: now,
            kind,
            remaining_seconds: Some(remaining),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sint_core::state::StateFile;
    use std::io::Write;

    const FAST: Duration = Duration::from_millis(5);
    const T: Thresholds = Thresholds {
        warn: 1800,
        red: 600,
    };

    fn dir() -> (tempfile::TempDir, StateDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let state = StateDir(tmp.path().join("cache"));
        fs::create_dir_all(&state.0).unwrap();
        (tmp, state)
    }

    fn kinds(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    fn write_state(state: &StateDir, job_id: u64, remaining: Option<i64>) {
        write_state_at(state, job_id, remaining, now_epoch());
    }

    /// `write_state` stamped with a caller-chosen clock, so a test that
    /// asserts on the exact remaining seconds can observe with the same
    /// `now` it wrote — a second ticking over between the two reads
    /// `remaining - 1`, which is what made the walltime test flake in CI.
    fn write_state_at(state: &StateDir, job_id: u64, remaining: Option<i64>, now: i64) {
        state
            .write_state(&StateFile {
                job_id,
                name: None,
                node: "n1".into(),
                end_epoch: remaining.map(|r| now + r),
                remaining_seconds: remaining,
                updated_epoch: now,
            })
            .unwrap();
    }

    #[test]
    fn times_out_when_nothing_happens() {
        let (_tmp, state) = dir();
        let got = wait_for_event(&state, 7, &[], T, Duration::from_millis(30), FAST);
        assert_eq!(got, json!({"timed_out": true}));
    }

    #[test]
    fn returns_the_first_matching_line_appended_after_the_call() {
        let (_tmp, state) = dir();
        let log = state.events_file(7);
        // Already there when the call starts: not an event for this caller.
        fs::write(&log, "{\"ts\":1,\"kind\":\"old\"}\n").unwrap();
        let writer = {
            let log = log.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                let mut f = fs::OpenOptions::new().append(true).open(&log).unwrap();
                // Garbage, an unwanted kind, then the one we want, then more.
                f.write_all(b"not json\n{\"ts\":2,\"kind\":\"cpu\"}\n")
                    .unwrap();
                f.write_all(b"{\"ts\":3,\"kind\":\"walltime_warn\",\"remaining_seconds\":1799}\n")
                    .unwrap();
                f.write_all(b"{\"ts\":4,\"kind\":\"walltime_red\"}\n")
                    .unwrap();
            })
        };
        let got = wait_for_event(
            &state,
            7,
            &kinds(&["walltime_warn", "walltime_red"]),
            T,
            Duration::from_secs(5),
            FAST,
        );
        writer.join().unwrap();
        assert_eq!(got["ts"], 3);
        assert_eq!(got["kind"], "walltime_warn");
        assert_eq!(got["remaining_seconds"], 1799);

        // Any kind: the very next line, which is the old-but-unread one is
        // not replayed; a fresh call starts at the current end.
        let got = wait_for_event(&state, 7, &[], T, Duration::from_millis(30), FAST);
        assert_eq!(got, json!({"timed_out": true}));
    }

    #[test]
    fn partial_lines_wait_for_their_newline() {
        let (_tmp, state) = dir();
        let log = state.events_file(7);
        fs::write(&log, "").unwrap();
        let writer = {
            let log = log.clone();
            std::thread::spawn(move || {
                let mut f = fs::OpenOptions::new().append(true).open(&log).unwrap();
                f.write_all(b"{\"ts\":9,\"kind\":\"gpu\"").unwrap();
                std::thread::sleep(Duration::from_millis(40));
                f.write_all(b",\"util\":50}\n").unwrap();
            })
        };
        let got = wait_for_event(&state, 7, &[], T, Duration::from_secs(5), FAST);
        writer.join().unwrap();
        assert_eq!(got, json!({"ts": 9, "kind": "gpu", "util": 50}));
    }

    #[test]
    fn a_wait_that_begins_mid_line_still_gets_that_line() {
        // The race the test above ran into on CI: the writer's first half
        // landed before wait_for_event measured the file, so "the end" was
        // in the middle of a line and its second half came back alone.
        let (_tmp, state) = dir();
        let log = state.events_file(7);
        fs::write(
            &log,
            "{\"ts\":1,\"kind\":\"old\"}\n{\"ts\":9,\"kind\":\"gpu\"",
        )
        .unwrap();
        let writer = {
            let log = log.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(40));
                let mut f = fs::OpenOptions::new().append(true).open(&log).unwrap();
                f.write_all(b",\"util\":50}\n").unwrap();
            })
        };
        let got = wait_for_event(&state, 7, &[], T, Duration::from_secs(5), FAST);
        writer.join().unwrap();
        assert_eq!(got, json!({"ts": 9, "kind": "gpu", "util": 50}));

        // A log that ends cleanly starts at its end: nothing old is replayed.
        let got = wait_for_event(&state, 7, &[], T, Duration::from_millis(30), FAST);
        assert_eq!(got, json!({"timed_out": true}));
    }

    #[test]
    fn the_start_offset_backs_up_over_a_partial_line() {
        let (_tmp, state) = dir();
        let log = state.events_file(8);
        assert_eq!(start_of_unfinished_line(&log), 0, "absent");
        fs::write(&log, "").unwrap();
        assert_eq!(start_of_unfinished_line(&log), 0, "empty");
        fs::write(&log, "abc\n").unwrap();
        assert_eq!(start_of_unfinished_line(&log), 4, "clean end");
        fs::write(&log, "abc\nde").unwrap();
        assert_eq!(start_of_unfinished_line(&log), 4, "partial last line");
        fs::write(&log, "de").unwrap();
        assert_eq!(start_of_unfinished_line(&log), 0, "partial only line");
        // Longer than one scan chunk, newline well behind the tail.
        let long = format!("x\n{}", "y".repeat(10_000));
        fs::write(&log, &long).unwrap();
        assert_eq!(start_of_unfinished_line(&log), 2, "across chunks");
    }

    #[test]
    fn synthetic_walltime_events_from_the_state_file() {
        let (_tmp, state) = dir();
        let now = now_epoch();
        write_state_at(&state, 7, Some(5000), now);
        let mut watch = WalltimeWatch::new(&state, 7, T, now);
        assert_eq!(watch.last, Some(5000));

        // Still plenty: nothing.
        write_state_at(&state, 7, Some(3000), now);
        assert_eq!(watch.observe(&state, 7, now), None);

        // Crossing the warning line.
        write_state_at(&state, 7, Some(1800), now);
        let ev = watch.observe(&state, 7, now).expect("warn");
        assert_eq!(ev.kind, "walltime_warn");
        assert_eq!(ev.remaining_seconds, Some(1800));
        assert!(ev.to_value()["synthetic"].as_bool().unwrap());

        // Under it already: no repeat.
        write_state_at(&state, 7, Some(1700), now);
        assert_eq!(watch.observe(&state, 7, now), None);

        // Crossing red.
        write_state_at(&state, 7, Some(599), now);
        assert_eq!(
            watch.observe(&state, 7, now).map(|e| e.kind),
            Some("walltime_red")
        );

        // The session ends: the file goes, once.
        fs::remove_file(state.state_file(7)).unwrap();
        assert_eq!(watch.observe(&state, 7, now).map(|e| e.kind), Some("ended"));
        assert_eq!(watch.observe(&state, 7, now), None);
    }

    #[test]
    fn session_ended_is_an_alias_of_ended() {
        let (_tmp, state) = dir();
        let log = state.events_file(7);
        fs::write(&log, "").unwrap();
        let writer = {
            let log = log.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                let mut f = fs::OpenOptions::new().append(true).open(&log).unwrap();
                f.write_all(b"{\"ts\":5,\"kind\":\"ended\",\"reason\":\"walltime\"}\n")
                    .unwrap();
            })
        };
        let got = wait_for_event(
            &state,
            7,
            &kinds(&["session_ended"]),
            T,
            Duration::from_secs(5),
            FAST,
        );
        writer.join().unwrap();
        assert_eq!(got["kind"], "ended");
        assert_eq!(got["reason"], "walltime");
    }

    #[test]
    fn skipping_both_thresholds_in_one_poll_reports_red() {
        let (_tmp, state) = dir();
        write_state(&state, 7, Some(5000));
        let mut watch = WalltimeWatch::new(&state, 7, T, now_epoch());
        write_state(&state, 7, Some(10));
        assert_eq!(
            watch.observe(&state, 7, now_epoch()).map(|e| e.kind),
            Some("walltime_red")
        );
    }

    #[test]
    fn a_stale_state_file_says_nothing() {
        let (_tmp, state) = dir();
        let now = now_epoch();
        state
            .write_state(&StateFile {
                job_id: 7,
                name: None,
                node: "n1".into(),
                end_epoch: Some(now + 100),
                remaining_seconds: Some(100),
                updated_epoch: now - 1000,
            })
            .unwrap();
        let mut watch = WalltimeWatch::new(&state, 7, T, now);
        assert_eq!(watch.last, None);
        assert_eq!(watch.observe(&state, 7, now), None);
    }

    #[test]
    fn wait_for_event_end_to_end_with_synthetic_events() {
        let (_tmp, state) = dir();
        write_state(&state, 7, Some(5000));
        let writer = {
            let state = state.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                write_state(&state, 7, Some(1000));
            })
        };
        let got = wait_for_event(&state, 7, &[], T, Duration::from_secs(5), FAST);
        writer.join().unwrap();
        assert_eq!(got["kind"], "walltime_warn");
        assert_eq!(got["remaining_seconds"], 1000);
        assert_eq!(got["synthetic"], true);

        // Only the kinds asked for count: a warn crossing is not a red.
        write_state(&state, 7, Some(5000));
        let writer = {
            let state = state.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                write_state(&state, 7, Some(1000));
            })
        };
        let got = wait_for_event(
            &state,
            7,
            &kinds(&["walltime_red"]),
            T,
            Duration::from_millis(120),
            FAST,
        );
        writer.join().unwrap();
        assert_eq!(got, json!({"timed_out": true}));
    }

    #[test]
    fn a_real_log_silences_the_synthetic_events() {
        let (_tmp, state) = dir();
        write_state(&state, 7, Some(5000));
        fs::write(state.events_file(7), "").unwrap();
        write_state(&state, 7, Some(10));
        let got = wait_for_event(&state, 7, &[], T, Duration::from_millis(40), FAST);
        assert_eq!(got, json!({"timed_out": true}));
    }

    #[test]
    fn snapshot_file_states() {
        let (_tmp, state) = dir();
        let err = read_snapshot(&state, 7).unwrap_err();
        let text = err.0.content[0].as_text().unwrap().text.clone();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({"job_id": 7, "error": "no snapshot yet"})
        );
        assert_eq!(err.0.is_error, Some(true));

        fs::write(state.metrics_file(7), "[1,2]").unwrap();
        assert!(read_snapshot(&state, 7).is_err());
        fs::write(state.metrics_file(7), "{\"cpu\":{\"pct\":12}}").unwrap();
        assert_eq!(
            read_snapshot(&state, 7).unwrap(),
            json!({"cpu": {"pct": 12}})
        );
    }

    #[test]
    fn a_snapshot_report_carries_its_age_and_the_30s_staleness() {
        let (_tmp, state) = dir();
        fs::write(state.metrics_file(7), "{\"host\":\"n1\",\"ts\":1000}").unwrap();
        let fresh = snapshot_report(&state, 7, 1030).unwrap();
        assert_eq!((fresh.age_secs, fresh.stale), (30, false));
        let old = snapshot_report(&state, 7, 1031).unwrap();
        assert_eq!((old.age_secs, old.stale), (31, true));
        let v = serde_json::to_value(&old).unwrap();
        assert_eq!(v["host"], "n1");
        assert_eq!(v["age_secs"], 31);
        assert_eq!(v["stale"], true);
        // A wrongly-typed field is a malformed snapshot, not a panic.
        fs::write(state.metrics_file(7), "{\"ts\":\"soon\"}").unwrap();
        assert!(snapshot_report(&state, 7, 1031).is_err());
    }

    #[test]
    fn tools_carry_annotations() {
        let tools = SintServer::new().tool_router.list_all();
        let hints = |name: &str| {
            let t = tools.iter().find(|t| t.name == name).expect(name);
            t.annotations.clone().expect("annotations")
        };
        for ro in [
            "list_sessions",
            "session_status",
            "queue",
            "quota",
            "quota_top",
            "monitor_snapshot",
            "peek",
            "wait_for_event",
            "agent_context",
        ] {
            assert_eq!(hints(ro).read_only_hint, Some(true), "{ro}");
        }
        for d in ["cancel_session", "send"] {
            assert_eq!(hints(d).destructive_hint, Some(true), "{d}");
        }
        assert_eq!(hints("ensure_session").idempotent_hint, Some(true));
    }

    #[test]
    fn a_launch_goes_stale_when_its_path_is_repointed() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new, link) = (
            tmp.path().join("old"),
            tmp.path().join("new"),
            tmp.path().join("sinteractive"),
        );
        fs::write(&old, "1").unwrap();
        fs::write(&new, "2").unwrap();
        std::os::unix::fs::symlink(&old, &link).unwrap();
        let launch = Launch {
            resolved: fs::canonicalize(&link).unwrap(),
            path: link.clone(),
        };
        assert!(!launch.is_stale());
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&new, &link).unwrap();
        assert!(launch.is_stale());
        fs::remove_file(&link).unwrap();
        assert!(launch.is_stale(), "a path that no longer resolves");
    }

    #[test]
    fn tool_errors_are_error_results() {
        let e = ToolError::from(anyhow!("no sinteractive session named 'x'"));
        let r = e.into_call_tool_result().unwrap();
        let CallToolResponse::Complete(r) = r else {
            panic!("complete result expected");
        };
        assert_eq!(r.is_error, Some(true));
        assert_eq!(r.structured_content, None);
        assert_eq!(
            r.content[0].as_text().unwrap().text,
            "no sinteractive session named 'x'"
        );
    }
}
