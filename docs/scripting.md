# Scripting and agent use

`sinteractive` has a headless mode designed for scripts and coding agents such
as [Claude Code](https://code.claude.com/docs/):

```bash
# Launch without attaching; returns once the session is ready
sinteractive --detach -n mywork --time=8h

# Get-or-create in one idempotent call
sinteractive --ensure mywork --time=8h --json
# {..., "created": true}    launched it
# {..., "created": false}   one was already running

# Machine-readable session info
sinteractive --list --json
sinteractive --status mywork --json

# Re-check the budget against Slurm now (e.g. right after a scontrol change)
sinteractive --refresh mywork --json
# {"job_id":147845,"name":"mywork","state":"RUNNING","node":"compute20",
#  "partition":"rna","cpus":8,"memory":"32G","memory_mb":32768,"gpus":0,
#  "time_limit":"8:00:00","elapsed":"0:43","end_epoch":1783180952,
#  "remaining_seconds":28757}
```

`--status --json` and `--list --json` return the same shape, except that
`--list` also carries `cwd` — which costs an SSH round-trip per session, so it
stays out of the single-session path that agents poll.

## A session is not a compute target

This is the part that most often goes wrong. A session is an orchestration
shell: it defaults to the `interactive` partition, the smallest on the
cluster, and it is shared with the shell the user is typing in. Editing, git,
and scheduler queries belong there. Everything heavier gets its own
allocation:

```bash
# One-off job — blocks, streams output, propagates the exit code
srun -p rna -c 8 --mem 32G -t 1:00:00 -- make test

# Sustained work — hold one allocation and reuse it
salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00
# salloc: Granted job allocation 244001    <- on STDERR, not stdout
srun --overlap --jobid=244001 -- cargo build --release
srun --overlap --jobid=244001 -- cargo test
scancel 244001
```

Capture the allocation id through `2>&1`:

```bash
ID=$(salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00 2>&1 |
     sed -n 's/.*Granted job allocation \([0-9]*\).*/\1/p')
```

Every `SLURM_*` variable is stripped from a session, so that tools inside it
(snakemake, nextflow) don't believe they are running as a job step. A useful
consequence: `srun` and `salloc` run from inside a session create their own
allocations rather than steps of the session's job.

The `cpus`, `memory`, `memory_mb` and `gpus` fields in `--status`/`--list`
JSON describe the *session's* allocation. `cpus` is what Slurm **allocated**,
which can exceed the request — on a cluster that hands out whole cores, a
`-j 1` session reports 2. That is the number you actually have. They are there to help size a
separate allocation — they are deliberately not exported into the session
environment, because a `SINTERACTIVE_CPUS` sitting in the environment is an
invitation to run `make -j` in the wrong place.

## Time budget

Inside a session, `SINTERACTIVE_JOB_ID` (and `SINTERACTIVE_NAME`, if named)
are exported, and `sinteractive --status` with no argument reports on the
current session. A state file at `~/.cache/sinteractive/JOBID.json` carries
`remaining_seconds`, so tools can poll the time budget without querying the
scheduler; it is removed when the session ends. The end time is re-checked
against Slurm immediately before every write, so `updated_epoch` is when the
whole snapshot was confirmed: if it is more than ~2 minutes old, treat the
file as stale and fall back to `sinteractive --status`. A walltime change made
with `scontrol update JobId=... TimeLimit=...` appears within about 30
seconds, or at once with `sinteractive --refresh`. When `squeue` cannot be
reached the file is left untouched rather than restamped, so it ages honestly
instead of vouching for a budget nobody verified; age it exactly with
`remaining_seconds - (now - updated_epoch)`.

In-session renames (`Ctrl-b $`) are reflected in the state file, `--status`,
and new panes, but shells already running keep their original
`SINTERACTIVE_NAME`.

## Claude Code integration

Install the skill and hooks:

```bash
sinteractive --install-claude   # from any installed copy
make claude-install             # equivalent, from a checkout
```

Both write `~/.claude/skills/bodhi-compute` and `~/.claude/hooks/`, then print
a `settings.json` block to merge. Neither edits the file, since yours probably
already has hooks in it — and a bad merge would silently disable every setting
in it.

`make install` puts the assets in `<prefix>/share/sinteractive` beside the
script, and `--install-claude` finds them relative to its own location. So it
works on a cluster where an admin ran `make nodes` and you never cloned the
repo. Point `SINTERACTIVE_SHARE` at a checkout to override.

**The [skill](https://code.claude.com/docs/en/skills)** teaches agents cluster
etiquette: neither the login node nor an sinteractive session is a compute
target, real work goes into an allocation sized for it, reuse sessions rather
than piling them up, check the time budget before long jobs, and clean up.

**`sinteractive --agent-context`** prints a briefing on the current session —
job, node, partition, allocation size, walltime remaining, and the rules
above. It exits 1 outside a session. Run it by hand to see exactly what an
agent is being told.

**Two hooks** wire that into an agent running inside a session:

| Hook | Event | What it does |
|---|---|---|
| `sinteractive-session-context.sh` | `SessionStart` | Emits `--agent-context` so the agent starts out knowing where it is |
| `sinteractive-walltime-guard.sh` | `UserPromptSubmit` | Silent until the session drops below `SINTERACTIVE_AGENT_WARN` seconds remaining (default 1800), then warns that long work won't survive |

Both exit 0 in every case, including outside a session, so they are harmless
on the login node and in unrelated projects. The guard prefers the cached
state file and only falls back to the scheduler when it is stale, so a quiet
session costs nothing.

Hooks fire at turn and tool boundaries, so work already in flight cannot be
warned about — put long work in its own allocation, which outlives the
session, rather than relying on the guard.
