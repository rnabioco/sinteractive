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
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use rmcp::handler::server::tool::{IntoCallToolResult, ToolRouter};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{
    CallToolResponse, CallToolResult, ContentBlock, ErrorData, Implementation,
    ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, RoleServer, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sint_core::now_epoch;
use sint_core::quota::QuotaSnapshot;
use sint_core::session::SessionInfo;
use sint_core::state::StateDir;

use super::agent_context::briefing;
use super::cancel::{cancel_job, CancelResult};
use super::common::Ctx;
use super::ensure::{ensure_data, Ensured};
use super::list::{list_data, ListRow};
use super::peek::{dump_screen, tail_lines};
use super::queue::{queue_data, QueueReport};
use super::quota::quota_data;
use super::send::send_command;
use crate::cli::LaunchArgs;

/// What the client is told at `initialize`.
const INSTRUCTIONS: &str =
    "sinteractive keeps persistent shells (zellij sessions) on Slurm compute \
nodes. A session is an orchestration shell, not a compute target: it is a small allocation shared \
with the shell the user is typing in, so editing, git and scheduler queries belong there and \
anything heavier gets its own srun/salloc allocation. Call session_status before long work to \
read the remaining walltime, and wait_for_event (which blocks until something happens) instead \
of polling. peek reads a session's screen; send types into the user's live shell, so only do that \
when asked.";

/// Default and cap for `wait_for_event`'s timeout, in seconds.
const WAIT_DEFAULT_SECS: u64 = 300;
const WAIT_MAX_SECS: u64 = 3600;
/// How often the event tail looks again.
const WAIT_POLL: Duration = Duration::from_secs(1);

/// The URI scheme the resources live under.
const URI_SESSIONS: &str = "sinteractive://sessions";

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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendParams {
    /// JOBID or session NAME; must be RUNNING.
    pub target: String,
    /// Typed into the session's shell, followed by Enter.
    pub command: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct QuotaParams {
    /// Probe the quota daemons now instead of reading the cache.
    pub check: Option<bool>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WaitParams {
    /// JOBID or session NAME; omitted means the current session.
    pub target: Option<String>,
    /// Event kinds to wait for (any kind when omitted or empty), e.g.
    /// `walltime_warn`, `walltime_red`, `session_ended`.
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
pub struct SendResult {
    pub job_id: u64,
    pub sent: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Briefing {
    pub text: String,
}

// ---- the server ---------------------------------------------------------

pub struct SintServer {
    ctx: Arc<Ctx>,
    tool_router: ToolRouter<Self>,
}

impl SintServer {
    pub fn new() -> Self {
        SintServer {
            ctx: Arc::new(Ctx::new()),
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
        description = "The user's RUNNING sinteractive sessions (the `sinteractive list --json` rows)."
    )]
    async fn list_sessions(&self) -> Result<Json<SessionList>, ToolError> {
        let sessions = self.blocking(|ctx| Ok(list_data(ctx)?)).await?;
        Ok(Json(SessionList { sessions }))
    }

    #[tool(
        name = "session_status",
        description = "One session's status, including remaining walltime (`sinteractive status --json`). Errors with {\"job_id\",\"state\":\"NOT_FOUND\"} when Slurm no longer knows the job."
    )]
    async fn session_status(
        &self,
        Parameters(p): Parameters<TargetParams>,
    ) -> Result<Json<SessionInfo>, ToolError> {
        let info = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, p.target.as_deref())?;
                session_or_not_found(ctx, job_id)
            })
            .await?;
        Ok(Json(info))
    }

    #[tool(
        name = "ensure_session",
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
        description = "The latest resource snapshot of a session's node (CPU, memory, GPU, processes), as its in-session sampler last wrote it. Errors with `no snapshot yet` until the first sample lands."
    )]
    async fn monitor_snapshot(
        &self,
        Parameters(p): Parameters<TargetParams>,
    ) -> ToolResult<CallToolResult> {
        let snapshot = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, p.target.as_deref())?;
                read_snapshot(&ctx.state, job_id)
            })
            .await?;
        Ok(CallToolResult::structured(snapshot))
    }

    #[tool(
        name = "peek",
        description = "The last lines of a RUNNING session's screen, read over ssh (`sinteractive peek`)."
    )]
    async fn peek(
        &self,
        Parameters(p): Parameters<PeekParams>,
    ) -> Result<Json<PeekResult>, ToolError> {
        let result = self
            .blocking(move |ctx| {
                let job_id = resolve(ctx, Some(&p.target))?;
                let session = ctx.require_running(job_id)?;
                let screen = dump_screen(ctx, &session)?;
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
        description = "Type a command into a RUNNING session's shell and press Enter (`sinteractive send`). This is the user's live shell: only when asked."
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
                send_command(ctx, &session, &p.command)?;
                Ok(SendResult { job_id, sent: true })
            })
            .await?;
        Ok(Json(result))
    }

    #[tool(
        name = "agent_context",
        description = "The briefing for an agent running inside a session: job, node, allocation, remaining walltime and the rules of the road (`sinteractive agent-context`). Errors outside a session."
    )]
    async fn agent_context(&self) -> Result<Json<Briefing>, ToolError> {
        let text = self.blocking(|ctx| Ok(briefing(ctx)?)).await?;
        Ok(Json(Briefing { text }))
    }

    #[tool(
        name = "quota",
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
        name = "wait_for_event",
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
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "sinteractive",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let rows = self
            .blocking(|ctx| Ok(list_data(ctx)?))
            .await
            .map_err(internal)?;
        let mut resources = vec![Resource::new(URI_SESSIONS, "sessions")
            .with_description("The user's running sessions (`sinteractive list --json`)")
            .with_mime_type("application/json")];
        for row in &rows {
            let id = row.info.job_id;
            let label = match &row.info.name {
                Some(name) => format!("{id} ({name})"),
                None => id.to_string(),
            };
            for (kind, what) in [
                ("status", "status and remaining walltime"),
                ("notices", "active notices"),
                ("metrics", "latest resource snapshot"),
            ] {
                resources.push(
                    Resource::new(
                        format!("{URI_SESSIONS}/{id}/{kind}"),
                        format!("{label} {kind}"),
                    )
                    .with_description(format!("Session {label}: {what}"))
                    .with_mime_type("application/json"),
                );
            }
        }
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let templates = [
            ("status", "One session's status object"),
            ("notices", "One session's active notices"),
            ("metrics", "One session's latest resource snapshot"),
        ]
        .into_iter()
        .map(|(kind, what)| {
            ResourceTemplate::new(
                format!("{URI_SESSIONS}/{{job_id}}/{kind}"),
                format!("session {kind}"),
            )
            .with_description(what)
            .with_mime_type("application/json")
        })
        .collect();
        Ok(ListResourceTemplatesResult::with_all_items(templates))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let uri = request.uri.clone();
        let Some(target) = parse_resource_uri(&uri) else {
            return Err(ErrorData::resource_not_found(
                format!("no such resource: {uri}"),
                None,
            ));
        };
        let text = self
            .blocking(move |ctx| read_resource_text(ctx, target))
            .await
            .map_err(internal)?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri).with_mime_type("application/json")
        ])
        .into())
    }
}

/// A tool-shaped failure as a protocol error, for the resource methods.
fn internal(e: ToolError) -> ErrorData {
    let msg =
        e.0.content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
    ErrorData::internal_error(msg, None)
}

/// What a `sinteractive://` URI names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResourceTarget {
    Sessions,
    Session(u64, SessionResource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionResource {
    Status,
    Notices,
    Metrics,
}

fn parse_resource_uri(uri: &str) -> Option<ResourceTarget> {
    let rest = uri.strip_prefix(URI_SESSIONS)?;
    if rest.is_empty() || rest == "/" {
        return Some(ResourceTarget::Sessions);
    }
    let mut parts = rest.strip_prefix('/')?.split('/');
    let job_id: u64 = parts.next()?.parse().ok()?;
    let kind = match parts.next()? {
        "status" => SessionResource::Status,
        "notices" => SessionResource::Notices,
        "metrics" => SessionResource::Metrics,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(ResourceTarget::Session(job_id, kind))
}

fn read_resource_text(ctx: &Ctx, target: ResourceTarget) -> ToolResult<String> {
    let value = match target {
        ResourceTarget::Sessions => serde_json::to_value(list_data(ctx)?),
        ResourceTarget::Session(job_id, SessionResource::Status) => {
            match ctx.session_info(job_id)? {
                Some(info) => serde_json::to_value(info),
                None => Ok(SessionInfo::not_found(job_id)),
            }
        }
        ResourceTarget::Session(job_id, SessionResource::Notices) => {
            serde_json::to_value(sint_core::notices::read(&ctx.state, job_id))
        }
        ResourceTarget::Session(job_id, SessionResource::Metrics) => {
            Ok(read_snapshot(&ctx.state, job_id)?)
        }
    };
    value
        .map(|v| v.to_string())
        .map_err(|e| ToolError::msg(format!("could not serialise the resource: {e}")))
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

// ---- wait_for_event -----------------------------------------------------

/// Remaining-walltime thresholds for the synthetic events, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub warn: i64,
    pub red: i64,
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
/// `session_ended`. Crossing means a transition since the call started, so
/// a session already under a threshold does not fire it again on every
/// call. On timeout the result is `{"timed_out":true}`.
///
/// The file is polled every `poll`; no inotify, so it works on the shared
/// filesystems the cache lives on.
pub fn wait_for_event(
    state: &StateDir,
    job_id: u64,
    kinds: &[String],
    thresholds: Thresholds,
    timeout: Duration,
    poll: Duration,
) -> Value {
    let wanted = |kind: &str| kinds.is_empty() || kinds.iter().any(|k| k == kind);
    let log = state.events_file(job_id);
    let deadline = Instant::now() + timeout;
    let mut offset = fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
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
                    kind: "session_ended",
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
        let now = now_epoch();
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
    fn synthetic_walltime_events_from_the_state_file() {
        let (_tmp, state) = dir();
        write_state(&state, 7, Some(5000));
        let mut watch = WalltimeWatch::new(&state, 7, T, now_epoch());
        assert_eq!(watch.last, Some(5000));

        // Still plenty: nothing.
        write_state(&state, 7, Some(3000));
        assert_eq!(watch.observe(&state, 7, now_epoch()), None);

        // Crossing the warning line.
        write_state(&state, 7, Some(1800));
        let ev = watch.observe(&state, 7, now_epoch()).expect("warn");
        assert_eq!(ev.kind, "walltime_warn");
        assert_eq!(ev.remaining_seconds, Some(1800));
        assert!(ev.to_value()["synthetic"].as_bool().unwrap());

        // Under it already: no repeat.
        write_state(&state, 7, Some(1700));
        assert_eq!(watch.observe(&state, 7, now_epoch()), None);

        // Crossing red.
        write_state(&state, 7, Some(599));
        assert_eq!(
            watch.observe(&state, 7, now_epoch()).map(|e| e.kind),
            Some("walltime_red")
        );

        // The session ends: the file goes, once.
        fs::remove_file(state.state_file(7)).unwrap();
        assert_eq!(
            watch.observe(&state, 7, now_epoch()).map(|e| e.kind),
            Some("session_ended")
        );
        assert_eq!(watch.observe(&state, 7, now_epoch()), None);
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
    fn resource_uris() {
        assert_eq!(
            parse_resource_uri("sinteractive://sessions"),
            Some(ResourceTarget::Sessions)
        );
        assert_eq!(
            parse_resource_uri("sinteractive://sessions/"),
            Some(ResourceTarget::Sessions)
        );
        assert_eq!(
            parse_resource_uri("sinteractive://sessions/42/status"),
            Some(ResourceTarget::Session(42, SessionResource::Status))
        );
        assert_eq!(
            parse_resource_uri("sinteractive://sessions/42/notices"),
            Some(ResourceTarget::Session(42, SessionResource::Notices))
        );
        assert_eq!(
            parse_resource_uri("sinteractive://sessions/42/metrics"),
            Some(ResourceTarget::Session(42, SessionResource::Metrics))
        );
        assert_eq!(
            parse_resource_uri("sinteractive://sessions/web/status"),
            None
        );
        assert_eq!(parse_resource_uri("sinteractive://sessions/42/nope"), None);
        assert_eq!(
            parse_resource_uri("sinteractive://sessions/42/status/x"),
            None
        );
        assert_eq!(parse_resource_uri("sinteractive://sessions/42"), None);
        assert_eq!(parse_resource_uri("file:///etc/passwd"), None);
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
