# Scripting and agent use

`sinteractive` has a headless mode designed for scripts and coding agents such
as [Claude Code](https://code.claude.com/docs/):

```bash
# Launch without attaching; returns once the session is ready
sinteractive --detach -n mywork --time=8h

# Get-or-create in one idempotent call
sinteractive ensure mywork --time=8h --json
# {..., "created": true}    launched it
# {..., "created": false}   one was already running

# Machine-readable session info
sinteractive list --json
sinteractive status mywork --json

# Re-check the budget against Slurm now (e.g. right after a scontrol change)
sinteractive refresh mywork --json
# {"job_id":147845,"name":"mywork","state":"RUNNING","node":"compute20",
#  "partition":"rna","cpus":8,"memory":"32G","memory_mb":32768,"gpus":0,
#  "time_limit":"8:00:00","elapsed":"0:43","end_epoch":1783180952,
#  "remaining_seconds":28757}

# Read the session's screen, or type into it
sinteractive peek mywork -n 40
sinteractive send mywork 'make test'

# Everything else that reports has --json too
sinteractive queue --json
sinteractive monitor mywork --json
sinteractive quota --json
sinteractive cancel mywork --json
```

`status --json` and `list --json` return the same shape, except that `list`
also carries `cwd` — which costs an SSH round-trip per session, so it stays
out of the single-session path that agents poll. `sinteractive schema` dumps
the JSON schemas of the session object, the state file, the quota snapshot
and a notice.

Exit codes follow 0.x: 0 success, 1 not found or failure, 2 usage. The 0.x
top-level flags (`--status`, `--list`, `--ensure`, …) still work for one
release with a warning on stderr.

## A session is not a compute target

This is the part that most often goes wrong. A session is an orchestration
shell: it defaults to the `interactive` partition, the smallest on the
cluster, and it is shared with the shell the user is typing in. Editing, git,
and scheduler queries belong there. Everything heavier gets its own
allocation:

```bash
# One-off job — blocks, streams output, propagates the exit code
srun -p rna -c 8 --mem 32G -t 1:00:00 -J make-test --comment=make-test -- make test

# Sustained work — hold one allocation and reuse it
salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00 -J cargo-ci --comment=cargo-ci
# salloc: Granted job allocation 244001    <- on STDERR, not stdout
srun --overlap --jobid=244001 -- cargo build --release
srun --overlap --jobid=244001 -- cargo test
scancel 244001
```

Capture the allocation id through `2>&1`:

```bash
ID=$(salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00 \
       -J cargo-ci --comment=cargo-ci 2>&1 |
     sed -n 's/.*Granted job allocation \([0-9]*\).*/\1/p')
```

Give every job a short descriptive name in both Slurm fields — `-J NAME` and
`--comment=NAME`, the same value — so a queue shared with other people says
what is running and why:

```bash
squeue --me -o "%.10i %.20j %.20P %.10M %k"   # %j is the name, %k the comment
```

Left unset, a job takes the command's basename and an empty comment, which is
how a partition ends up full of jobs called `bash`. Both fields are worth
filling because they survive differently: the comment is readable on a live
job (`squeue`, `scontrol show job ID`) but is only kept in accounting when the
cluster sets `AccountingStoreFlags=job_comment` — Bodhi does not, so `sacct`
history shows the name alone. Name and comment belong to the allocation, so
naming the `salloc` covers every `srun --overlap` step run inside it.

Every `SLURM_*` variable is stripped from a session, so that tools inside it
(snakemake, nextflow) don't believe they are running as a job step. A useful
consequence: `srun` and `salloc` run from inside a session create their own
allocations rather than steps of the session's job.

The `cpus`, `memory`, `memory_mb` and `gpus` fields in `status`/`list`
JSON describe the *session's* allocation. `cpus` is what Slurm **allocated**,
which can exceed the request — on a cluster that hands out whole cores, a
`-j 1` session reports 2. That is the number you actually have. They are
there to help size a separate allocation — they are deliberately not exported
into the session environment, because a `SINTERACTIVE_CPUS` sitting in the
environment is an invitation to run `make -j` in the wrong place.

## Time budget

Inside a session, `SINTERACTIVE_JOB_ID` (and `SINTERACTIVE_NAME`, if named)
are exported, and `sinteractive status` with no argument reports on the
current session. A state file at `<cache>/JOBID.json` (`SINTERACTIVE_CACHE`,
default `~/.cache/sinteractive`) carries `remaining_seconds`, so tools can
poll the time budget without querying the scheduler; it is removed when the
session ends:

```json
{"job_id":147845,"name":"mywork","node":"compute20",
 "end_epoch":1783180952,"remaining_seconds":869,"updated_epoch":1783180083}
```

The end time is re-checked against Slurm immediately before every write, so
`updated_epoch` is when the whole snapshot was confirmed: if it is more than
~2 minutes old, treat the file as stale and fall back to `sinteractive
status`. A walltime change made with `scontrol update JobId=...
TimeLimit=...` appears within about 30 seconds (`SINTERACTIVE_POLL`), or at
once with `sinteractive refresh`. When `squeue` cannot be reached the file is
left untouched rather than restamped, so it ages honestly instead of vouching
for a budget nobody verified; age it exactly with `remaining_seconds - (now -
updated_epoch)`.

The schema and field order of this file are frozen — it is the 0.x contract,
unchanged.

## Events and metrics

The in-session sampler writes two more files beside the state file, both
readable from the login node without ssh.

**`<cache>/JOBID.metrics.json`** is the latest host snapshot — CPU and memory
against the job's cgroup limits, load, GPUs and the busiest processes —
refreshed every few seconds. `sinteractive monitor TARGET --json` prints it
once (`no snapshot yet` before the first sample, or when it is more than
30 s old); without `--json` and with a tty it is the nvitop-style TUI.
`sinteractive snapshot --json` takes the same sample of whatever host it runs
on, scoped to the Slurm job it runs in, and `monitor --live HOST` runs that
over ssh every 2 s.

**`<cache>/JOBID.events.ndjson`** is the session's event log, one
`{"ts": …, "kind": …, …}` object per line. `sinteractive events [TARGET]`
prints it; `--follow` keeps streaming as lines are appended, `--since EPOCH`
starts from a point in time. The kinds are the ones the MCP server's
`wait_for_event` matches on — `walltime_warn`, `walltime_red`,
`session_ended` among them — and that tool reads this file.

## Claude Code integration

Install the skills and register the hooks, statusline and MCP server:

```bash
sinteractive install-claude   # from any installed copy
make claude-install           # equivalent, from a checkout
```

It writes every skill under `~/.claude/skills/`, then merges the hooks and
the statusline into `~/.claude/settings.json` and registers the MCP server
with `claude mcp add`. Skills are discovered from what ships beside the
binary (`<prefix>/share/sinteractive`, or the checkout named by
`SINTERACTIVE_SHARE`) rather than named in the installer, so a new one
arrives with an upgrade and needs no new flag. The settings file is yours and
usually already has hooks in it, so the merge guarantees:

- **Additive.** Entries are appended to whatever `.hooks` already holds;
  nothing else in the file is touched, and key order survives.
- **Idempotent.** A hook is skipped when it is already registered in
  `settings.json` or `settings.local.json`, and a half-registered pair gets
  only its missing half. When the merge changes nothing, nothing is written.
- **Recoverable.** The new file is written through a temp file beside the
  original, and the version it replaces is kept as
  `settings.json.bak-<stamp>`. A symlinked `settings.json` is resolved first,
  so a dotfiles repo gets its target edited rather than its link replaced.
- **Cautious.** A `settings.json` that does not parse is reported and left
  alone rather than merged into, and the block to merge by hand is printed.

The 0.x hook scripts (`~/.claude/hooks/sinteractive-*.sh`) are removed and
their entries replaced with the native subcommands; stale `bodhi-*` skills
are removed when their `hpc-*` successor is installed. Exit codes: 0 done,
1 no assets found, 2 a settings file was refused (everything else was still
installed).

**Six [skills](https://code.claude.com/docs/en/skills)** teach agents how work
is done here. Skills load on demand from their descriptions, so an agent picks
up the one the task calls for rather than carrying all six. The three `hpc-*`
skills go one step further: their SKILL.md holds the rules shared by both
clusters this tool runs on and delegates the rest to an `alpine.md` or
`bodhi.md` beside it, so the agent reads the system it is actually on and is
never fed the other one's partitions, paths, and quotas.

`hpc-compute` covers cluster etiquette: neither the login node nor an
sinteractive session is a compute target, real work goes into an allocation
sized for it, reuse sessions rather than piling them up, check the time budget
before long jobs, observe a session with `peek`/`send`, and clean up.

`slurm-discovery` covers finding out what the cluster offers rather than
assuming it: what the partitions are and how big, which accounts and QOS you
hold, and the rule that decides whether a given combination is submittable —
your account in the partition's `AllowAccounts`, your QOS in both its
`AllowQos` and your own association. It also covers reading `squeue`'s reason
column when a job is refused or sits `PENDING`, and caches the answers per
cluster so the survey is run once rather than every session.

`hpc-storage` covers where data goes, on both clusters this tool runs on. On
Bodhi, `/beevol` is one shared BeeGFS mount and the compute node's `/tmp` is
a local disk, so inputs are read from the former and scratch is written to
the latter and cleaned up on exit. On Alpine (CU Boulder), the layout is
tiered the other way around: a 2 GB `/home` that nothing may be written to,
a small backed-up `/projects`, and a huge purged `/scratch/alpine` parallel
filesystem where all work runs. It also warns that `du` on a home directory
can run for minutes, and that a shared filesystem is full enough for a large
write to be somebody else's problem too.

`hpc-software` covers how to get a tool: the module tree first — Bodhi ships
around 137 preinstalled bioinformatics packages under Tcl modules, Alpine a
general-HPC Lmod hierarchy — then a container, then `pixi`/`uv` for the
remainder. Pin the version, load inside the job script rather than the login
shell, and never `pip install` into the system Python.

`slurm-batch` covers work that is per-sample rather than a single command:
`sbatch` scripts, arrays and why to throttle them with `%N`, dependency
chains, and using `sacct` to size the next run from what the last one actually
used — noting that `MaxRSS` lives on the step rows, where `sacct -X` will not
show it.

`git-workflow` covers the git conventions, and is about the repository open in
the session rather than the cluster: semantic versioning with annotated
`vX.Y.Z` tags, Conventional Commit messages, one worktree per branch under
`.claude/worktrees/`, landing work through a pull request rather than
committing to `main`, and running the repo's own CI gates before pushing.

**`sinteractive agent-context`** prints a briefing on the current session —
job, node, partition, allocation size, walltime remaining, and the rules
above. It exits 1 outside a session. Run it by hand to see exactly what an
agent is being told.

**Two hooks** wire that into an agent running inside a session. Both are
subcommands of the binary, so there are no scripts to keep in step:

| Hook | Event | What it does |
|---|---|---|
| `sinteractive hook session-start` | `SessionStart` | Emits the `agent-context` briefing so the agent starts out knowing where it is |
| `sinteractive hook prompt` | `UserPromptSubmit` | Silent until the session drops below `SINTERACTIVE_AGENT_WARN` seconds remaining (default 1800), then warns that long work won't survive |

Both exit 0 in every case, including outside a session, so they are harmless
on the login node and in unrelated projects. The guard prefers the cached
state file, aged exactly, and only falls back to the scheduler when it is
missing or stale, so a quiet session costs nothing. Hooks fire at turn and
tool boundaries, so work already in flight cannot be warned about — put long
work in its own allocation, which outlives the session, rather than relying
on the guard.

**The statusline.** `sinteractive statusline` is registered as Claude Code's
`statusLine` command (`refreshInterval` 5). It shows `⏺ Opus · ctx 42% ·
~/proj` on a login node and, inside a session, adds `· sint 147845 mywork ·
2h41m · ⚠1` — the remaining walltime and the notice count, read from the
cache files only, so a 5-second refresh never touches the scheduler. Theme
follows `SINTERACTIVE_THEME` and Claude Code's own dark/light palette.

While Claude Code is running in a session whose hooks are not registered yet,
a `sinteractive install-claude` hint joins the session's notices — the
`⚠ N notices` counter on the status bar, read in full with `Ctrl+b n` or
`sinteractive status`. It is gated on a live `claude` process, so it never
appears for people who don't use Claude Code, and it clears once the hooks
are registered.

## MCP server

`sinteractive mcp` is a [Model Context Protocol](https://modelcontextprotocol.io/)
server over stdio, so an agent can ask about sessions through typed tools
instead of shelling out and parsing. `sinteractive install-claude` registers
it for Claude Code; by hand, the equivalent is

```bash
claude mcp add --scope user sinteractive -- sinteractive mcp
```

which lands in the Claude settings as

```json
"mcpServers": { "sinteractive": { "type": "stdio", "command": "sinteractive", "args": ["mcp"] } }
```

Every tool calls the same code as the corresponding `--json` command and
returns that command's JSON as the tool's structured content (with a matching
`outputSchema`), so the shapes documented above are the shapes the tools
return. Failures a caller should see — an unknown name, a session that is not
running, no snapshot yet — come back as tool results with `isError: true`
carrying the CLI's wording or its JSON error object; only a malformed request
is a protocol error. The server's stderr is its log: `ensure_session`
narrates the launch there exactly as `sinteractive ensure` does, and nothing
but JSON-RPC ever goes to stdout.

| Tool | Arguments | Returns |
|---|---|---|
| `list_sessions` | — | `{"sessions": [...]}` — the `list --json` rows (`cwd` included) |
| `session_status` | `target?` | the `status --json` object; the `NOT_FOUND` object as an error |
| `ensure_session` | `name`, `time?`, `cpus?`, `mem?`, `partition?`, `sbatch_args?` | the `ensure --json` object, `created` set |
| `cancel_session` | `target` | `{"job_id", "cancelled"}` |
| `queue` | `all?` | the `queue --json` object |
| `monitor_snapshot` | `target?` | the session's latest `<jobid>.metrics.json`; `no snapshot yet` before the first sample |
| `peek` | `target`, `lines?` | `{"job_id", "node", "lines": [...]}` |
| `send` | `target`, `command` | `{"job_id", "sent": true}` — the user's live shell, so only when asked |
| `agent_context` | — | `{"text": ...}`, the `agent-context` briefing |
| `quota` | `check?` | the `quota --json` object; `{"error": "quota unavailable"}` as an error |
| `wait_for_event` | `target?`, `kinds?`, `timeout_secs?` | the next matching event, or `{"timed_out": true}` |

`target` is a JOBID or session NAME; omitted, it is the session the server
runs inside (`SINTERACTIVE_JOB_ID`), which is the case for an agent started
in a session.

`wait_for_event` blocks — default 300 s, at most 3600 — until a line matching
one of `kinds` (any kind when omitted) is appended to the session's event log
(`<cache>/JOBID.events.ndjson`, one `{"ts": …, "kind": …, …}` object per
line, written by the in-session sampler) and returns that line. Only lines
appended after the call started count, so it is a wait, not a replay. While
the session has no event log, the state file stands in: `remaining_seconds`
crossing 1800 or 600 (`SINTERACTIVE_AGENT_WARN` / `SINTERACTIVE_WARN_RED`)
yields a synthetic `walltime_warn` / `walltime_red` event, and the state file
disappearing yields `session_ended`; those carry `"synthetic": true`. Use it
in place of polling `session_status`.

The server also exposes read-only resources: `sinteractive://sessions` (the
list) and, per session, `sinteractive://sessions/JOBID/status`, `.../notices`
and `.../metrics`, each `application/json`.
