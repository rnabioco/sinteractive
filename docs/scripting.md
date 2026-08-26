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

Install the skills and hooks:

```bash
sinteractive --install-claude   # from any installed copy
make claude-install             # equivalent, from a checkout
```

Both write every skill under `~/.claude/skills/` and the hooks to
`~/.claude/hooks/`, then register the hooks in `~/.claude/settings.json`.
Skills are discovered from what ships beside the script rather than named in
the installer, so a new one arrives with an upgrade and needs no new flag.
That settings file is yours and
usually already has hooks in it, so the merge is done by `jq` and only by
`jq` — string surgery on it in bash could silently disable every setting in
the file. What the merge guarantees:

- **Additive.** Entries are appended to whatever `.hooks` already holds;
  nothing else in the file is touched, and key order survives.
- **Idempotent.** A hook is skipped when a script of that name is already
  registered in `settings.json` or `settings.local.json` — matched by script
  name, so a hand-edited path or a dropped `bash ` prefix still counts, and a
  half-registered pair gets only its missing half. When the merge changes
  nothing, nothing is written.
- **Recoverable.** The new file is written through a temp file beside the
  original, and the version it replaces is kept as
  `settings.json.bak-<stamp>`. A symlinked `settings.json` is resolved first,
  so a dotfiles repo gets its target edited rather than its link replaced.
- **Cautious.** A `settings.json` that does not parse is reported and left
  alone rather than merged into.

Without `jq` on `$PATH` the block is printed to merge by hand, as before
(`pixi global install jq` is one way to get one).

`make install` puts the assets in `<prefix>/share/sinteractive` beside the
script, and `--install-claude` finds them relative to its own location. So it
works on a cluster where an admin ran `make install-system` and `make nodes`
and you never cloned the repo — both ship the assets, and the compute nodes
need them too, since running `--install-claude` from inside a session runs
the node's copy of the script. Point `SINTERACTIVE_SHARE` at a checkout to
override, and `make nodes-check` to see which nodes actually have them.

**Six [skills](https://code.claude.com/docs/en/skills)** teach agents how work
is done here. Skills load on demand from their descriptions, so an agent picks
up the one the task calls for rather than carrying all six.

`bodhi-compute` covers cluster etiquette: neither the login node nor an
sinteractive session is a compute target, real work goes into an allocation
sized for it, reuse sessions rather than piling them up, check the time budget
before long jobs, and clean up.

`slurm-discovery` covers finding out what the cluster offers rather than
assuming it: what the partitions are and how big, which accounts and QOS you
hold, and the rule that decides whether a given combination is submittable —
your account in the partition's `AllowAccounts`, your QOS in both its
`AllowQos` and your own association. It also covers reading `squeue`'s reason
column when a job is refused or sits `PENDING`, and caches the answers per
cluster so the survey is run once rather than every session.

`bodhi-storage` covers where data goes: `/beevol` is one shared BeeGFS mount
and the compute node's `/tmp` is a local disk, so inputs are read from the
former and scratch is written to the latter and cleaned up on exit. It also
warns that `du` on a home directory can run for minutes, and that the shared
filesystem is full enough for a large write to be somebody else's problem too.

`bodhi-software` covers how to get a tool: the module tree first — around 137
preinstalled packages, so most of a genomics pipeline is a `module load` away
— then a container, then `pixi`/`uv` for the remainder. Pin the version,
load inside the job script rather than the login shell, and never `pip
install` into the system Python.

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

While Claude Code is running in a session whose hooks are not registered yet,
the yellow rule between the pane and the status bar carries a centred,
scrolling `sinteractive --install-claude` notice. It is gated on a live `claude` process,
so it never appears for people who don't use Claude Code, and it clears once
the hooks are registered.

Hooks fire at turn and tool boundaries, so work already in flight cannot be
warned about — put long work in its own allocation, which outlives the
session, rather than relying on the guard.
