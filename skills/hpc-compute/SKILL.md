---
name: hpc-compute
description: Run compute work on the Bodhi and Alpine (CU Boulder/CURC) HPC clusters. Use whenever a task involves builds, analyses, pipelines, simulations, or any CPU-, memory-, or GPU-heavy or long-running command — such work must run inside a Slurm allocation sized for it, never on the login node and never in an sinteractive session. Covers deciding which cluster and node you are on, getting an allocation with srun or salloc, choosing the cluster's partitions and QOS, managing sinteractive sessions, checking time budgets, and observing the user's interactive sessions.
---

# Running compute work

## First: where am I?

Two clusters share this skill. Detect which, and read **only that cluster's
file** in this skill's directory for partitions, QOS, and local hazards —
the other cluster's details are noise:

```bash
[ -d /scratch/alpine ] && echo alpine || { [ -d /beevol ] && echo bodhi; }
```

- **Alpine** (CU Boulder / CURC) → read `alpine.md` next to this SKILL.md
- **Bodhi** → read `bodhi.md` next to this SKILL.md

`$SINTERACTIVE_JOB_ID` set → you are inside an sinteractive zellij session on
a compute node. Unset → you are on the login node.

**The rule is the same either way: that shell is for orchestration, not
compute.** Editing, git, `squeue`/`sinfo`, and other sub-CPU-minute commands
belong there. Everything heavier gets its own allocation, sized for the job.

An sinteractive session is *not* a compute target, even though it lives on a
compute node. It is usually a small allocation shared with the shell the
user is actively typing in. Do not run heavy commands in one, and do not
`srun --overlap` into one — you would be competing with the user for a small
allocation.

## Run work in its own allocation

`SLURM_*` is stripped from an sinteractive session, so `srun` and `salloc` run
from inside one create their own allocations rather than steps of the
session's job. Both stream stdout/stderr back and propagate the command's exit
code.

**Name every job in both fields.** Give each `srun` and `salloc` a short,
descriptive name and pass it twice — `-J NAME` and `--comment=NAME`, the same
value in both:

```bash
squeue --me -o "%.10i %.20j %.20P %.10M %k"   # %j is the name, %k the comment
```

Name the work, not the tool: `bwa-align`, not `job1` or `bash`. Left unset,
the job takes the command's basename and an empty comment, so a queue full of
`bash` and `uv` tells nobody sharing the partition what is running or why.
This mirrors how sinteractive tags its own sessions (`sint-NAME` as the job
name, `sinteractive:NAME` as the comment).

Where a cluster's accounting does not store the comment (Bodhi's does not),
it is readable on a live job (`squeue`, `scontrol show job ID`) but comes
back empty from `sacct` history — the name is the half that survives there.
That is the reason to fill both rather than picking one.

**One-off job** — blocks until it finishes (partition and QOS come from the
cluster file):

```bash
srun -p PART [--qos=QOS] -c 8 --mem 32G -t 1:00:00 -J make-test --comment=make-test -- make test
```

Use the Bash tool's background mode for long ones; `srun` stays attached for
the duration.

**Sustained or iterative work** — hold one allocation and reuse it, instead of
queueing separately for every command:

```bash
salloc --no-shell -p PART [--qos=QOS] -c 32 --mem 96G -t 4:00:00 -J cargo-ci --comment=cargo-ci
# salloc: Granted job allocation 244001      <- on STDERR, not stdout
srun --overlap --jobid=244001 -- cargo build --release
srun --overlap --jobid=244001 -- cargo test
scancel 244001                                # when done
```

Name and comment live on the allocation, so naming the `salloc` covers every
`srun --overlap` step run inside it.

`salloc --no-shell` returns immediately. Capture the job id through `2>&1`:

```bash
ID=$(salloc --no-shell -p PART -c 32 --mem 96G -t 4:00:00 \
       -J cargo-ci --comment=cargo-ci 2>&1 |
     sed -n 's/.*Granted job allocation \([0-9]*\).*/\1/p')
```

Work inside such an allocation is bounded by that allocation's own `-t`, not
by the sinteractive session's walltime. The session dying still ends any
`srun` you are streaming from, so for anything long prefer the background Bash
mode over holding the stream, or check both budgets.

**Always cancel an allocation you are done with.** A held `salloc` occupies
the nodes until its walltime expires.

Request only what the task needs, and ask the user before requesting more
than a day of walltime or a whole node's worth of resources. Partitions can
restrict which accounts and QOS may submit, so the right `-p` can still be
rejected — the `slurm-discovery` skill covers mapping that out, and reading
the reason when a job is refused or stuck `PENDING`.

### Check for reservations before asking for walltime

**A job asking for more walltime than remains before a maintenance
reservation does not fail — it is silently deferred to after the window.**
The job queues, looks normal, and the cost is invisible unless you check:

```bash
scontrol show reservation        # ACTIVE = on now; INACTIVE = scheduled
srun --test-only -p PART -c 4 --mem 8G -t 21:00:00 -- true
# srun: Job ... to start at <TIME>   <- the scheduler's real verdict, nothing queued
```

Size `-t` to fit in the gap before the window; if the work cannot fit, say
so and let the user decide between splitting it and waiting. A pending job
showing `ReqNodeNotAvail, Reserved for maintenance` is not stuck and does not
need resubmitting — shortening `-t` is what makes it run sooner. Nothing
running survives an all-node window, sinteractive sessions included; launch
sessions that end before it. Bodhi has a recurring monthly all-node window —
`bodhi.md` covers reading it in detail.

## sinteractive sessions

These are the user's persistent interactive shells. You mostly *observe* them;
create one only when the user wants a durable place to work, not as somewhere
to run a command.

```bash
sinteractive list --json
# [{"job_id":147845,"name":"agent","state":"RUNNING","node":"compute20",
#   "partition":"rna","cpus":8,"memory":"32G","memory_mb":32768,"gpus":0,
#   "elapsed":"0:43","time_limit":"4:00:00","end_epoch":1783180952,
#   "remaining_seconds":28757,"cwd":"~/devel/proj"}, ...]
```

Get-or-create is one idempotent call — no need to list, parse, and recover
from a duplicate-name error:

```bash
sinteractive ensure agent --time=4h -j 8 -m 32G --json
# {... ,"created":true}    launched it
# {... ,"created":false}   one was already running; this is the same object
```

`ensure` implies `--detach`, accepts the same launch options as a normal
launch, and passes unrecognized flags through to `sbatch`. A `PENDING` match
counts as existing and is returned with `"state":"PENDING"` rather than waited
on, so poll if you need it ready. Two concurrent `ensure` calls for the same
name can still both launch.

`list --json` and `status --json` return the same shape; `list`
additionally carries `cwd`, which costs an SSH round-trip per session. The
`cpus`/`memory`/`gpus` fields describe the *session's* allocation — use them
to size a separate allocation, never to size work run in the session.

Notes:

- Job caps are **per partition or per QOS**, not per user across the
  cluster. A "you already have N/N jobs" refusal means that one partition or
  QOS is full — launch the session elsewhere rather than cancelling
  somebody's session to free a slot.
- Never cancel a session you did not create without asking the user.
- Users can rename a session while it runs (`Ctrl-b $`), so resolve and cache
  sessions by `job_id`, not name.

## Check the time budget before long work

```bash
sinteractive status JOBID --json   # or NAME; includes remaining_seconds
```

Inside a session, `status` needs no target, and `sinteractive
agent-context` prints a briefing on the current session and these rules.

For frequent polling, read the state file instead of hitting the scheduler —
it is refreshed about every 30 s:

```bash
cat ~/.cache/sinteractive/JOBID.json
# {"job_id":147845,"name":"agent","node":"compute20",
#  "end_epoch":1783180952,"remaining_seconds":869,"updated_epoch":1783180083}
```

The end time is re-checked against Slurm immediately before every write, so
`updated_epoch` is when the whole snapshot was confirmed. If it is more than
~2 minutes old, treat the file as stale and fall back to `sinteractive
status`; age it exactly with `remaining_seconds - (now - updated_epoch)`.

**Re-check before long work; do not trust a budget you read earlier in the
conversation.** Wall time can change underneath you: the user may shorten a
job with `scontrol update JobId=... TimeLimit=...`, or an administrator may
extend one. A number read an hour ago is not evidence about now. After any
such change, `sinteractive refresh JOBID` makes the cached file agree
immediately instead of at the next poll:

```bash
sinteractive refresh JOBID --json   # re-check now; same output as status
```

Note that on most clusters an ordinary user can only *reduce* a job's
TimeLimit — raising it needs operator privileges, and `scontrol` fails with
`Access/permission denied` otherwise. If the user says they extended a job,
confirm it actually took effect rather than assuming it did.

## Observe or drive an interactive session

Sessions live in zellij on the compute node; `sinteractive` reaches them over
ssh for you, so there is no multiplexer socket or binary path to know.

To read what is on screen (last 100 lines; `-n` for more or fewer):

```bash
sinteractive peek JOBID|NAME [-n 100]
```

To type into it — this is the user's live shell, so only when asked:

```bash
sinteractive send JOBID|NAME 'command'
```

Both exit 1 with a message when the session is not running or the node
cannot be reached.

## Cleanup

Cancel allocations you created as soon as the work is done — `scancel ID` for
an `salloc`, `sinteractive cancel JOBID|NAME` for a session. Never cancel a
session you did not create without asking the user.

## Related skills

- `slurm-discovery` — which partitions, accounts and QOS you may actually use.
- `hpc-storage` — where work runs and output lands, per cluster.
- `hpc-software` — check the module tree before building or installing.
- `slurm-batch` — `sbatch`, arrays and dependencies, when it is not one job.
