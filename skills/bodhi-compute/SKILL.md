---
name: bodhi-compute
description: Run compute work on the Bodhi HPC cluster. Use whenever a task involves builds, analyses, pipelines, simulations, or any CPU-, memory-, or GPU-heavy or long-running command — such work must run inside a Slurm allocation sized for it, never on the login node and never in an sinteractive session. Covers deciding where you are, getting an allocation with srun or salloc, managing sinteractive sessions, checking time budgets, and observing the user's interactive sessions.
---

# Running compute work on Bodhi

## First: where am I?

`$SINTERACTIVE_JOB_ID` set → you are inside an sinteractive tmux session on a
compute node. Unset → you are on the login node.

**The rule is the same either way: that shell is for orchestration, not
compute.** Editing, git, `squeue`/`sinfo`, and other sub-CPU-minute commands
belong there. Everything heavier gets its own allocation, sized for the job.

An sinteractive session is *not* a compute target, even though it lives on a
compute node. It defaults to the `interactive` partition, which is the
smallest and least capable on the cluster, and it is usually a 2-CPU / 8G
allocation shared with the shell the user is actively typing in. Do not run
heavy commands in one, and do not `srun --overlap` into one — you would be
competing with the user for a small allocation.

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

Bodhi's `AccountingStoreFlags` does not include `job_comment`, so the comment
is readable on a live job (`squeue`, `scontrol show job ID`) but comes back
empty from `sacct` history — the name is the half that survives there. That is
the reason to fill both rather than picking one.

**One-off job** — blocks until it finishes:

```bash
srun -p rna -c 8 --mem 32G -t 1:00:00 -J make-test --comment=make-test -- make test
```

Use the Bash tool's background mode for long ones; `srun` stays attached for
the duration.

**Sustained or iterative work** — hold one allocation and reuse it, instead of
queueing separately for every command:

```bash
salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00 -J cargo-ci --comment=cargo-ci
# salloc: Granted job allocation 244001      <- on STDERR, not stdout
srun --overlap --jobid=244001 -- cargo build --release
srun --overlap --jobid=244001 -- cargo test
scancel 244001                                # when done
```

Name and comment live on the allocation, so naming the `salloc` covers every
`srun --overlap` step run inside it.

`salloc --no-shell` returns immediately. Capture the job id through `2>&1`:

```bash
ID=$(salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00 \
       -J cargo-ci --comment=cargo-ci 2>&1 |
     sed -n 's/.*Granted job allocation \([0-9]*\).*/\1/p')
```

Work inside such an allocation is bounded by that allocation's own `-t`, not
by the sinteractive session's walltime. The session dying still ends any
`srun` you are streaming from, so for anything long prefer the background Bash
mode over holding the stream, or check both budgets.

**Always cancel an allocation you are done with.** A held `salloc` occupies
the nodes until its walltime expires.

### Choosing a partition

```bash
sinfo -o "%20P %5a %10l %6D %6t %N"          # what exists, and what's idle
```

Pick the partition the work belongs to — `rna` for rnabioco work (6+ nodes,
usually several idle), `normal` as the general fallback, `bigmem` for
memory-heavy jobs, `gpu` for GPUs. **Never `interactive`** — it is reserved
for sinteractive sessions and is the smallest partition on the cluster
(~3 nodes).

Request only what the task needs, and ask the user before requesting more than
a day of walltime or a whole node's worth of resources.

`gpu` and some other partitions restrict which accounts and QOS may submit to
them, so the right `-p` can still be rejected under the wrong `-A`. The
`slurm-discovery` skill covers mapping that out, and reading the reason when a
job is refused or stuck `PENDING`.

### Check for a maintenance window before asking for walltime

Bodhi takes a maintenance reservation roughly once a month, and it covers
**every node on the cluster**. Check before requesting anything long:

```bash
scontrol show reservation
# ReservationName=monthly-maint StartTime=2026-08-27T06:00:00
#   EndTime=2026-08-28T06:00:00 Duration=1-00:00:00
#   Nodes=compgpu[01-03],compute[00-21] NodeCnt=25
#   Flags=MAINT,IGNORE_JOBS,SPEC_NODES,ALL_NODES
#   Users=root  State=INACTIVE
```

`State=INACTIVE` means it has not started yet; `ACTIVE` means it is on and the
nodes are gone. `Users=root` means it is not a reservation you can submit
into. `ALL_NODES` is the part that matters — there is nowhere else to go.

**A job asking for more walltime than remains before the window does not
fail. It gets deferred to after the window.** That is the whole hazard: the
job queues and looks normal, and the cost is invisible unless you check. With
the window 21h49m away:

```console
$ srun --test-only -p rna -c 4 --mem 8G -t 21:00:00 -- true
srun: Job 245072 to start at 2026-08-26T08:10:20 ...      # starts now

$ srun --test-only -p rna -c 4 --mem 8G -t 22:00:00 -- true
srun: Job 245073 to start at 2026-08-28T06:00:00 ...      # +46 hours
```

One extra hour of requested walltime cost two days of waiting. So **size `-t`
to fit in the gap**, and if the work genuinely cannot fit, say so and let the
user decide between splitting it and waiting.

How long is the gap:

```bash
scontrol show reservation 2>/dev/null |
  sed -n 's/.*ReservationName=\([^ ]*\) StartTime=\([^ ]*\) EndTime=\([^ ]*\).*/\1 \2 \3/p' |
  while read -r name start end; do
    s=$(date -d "$start" +%s) now=$(date +%s)
    ((s > now)) && printf '%s starts in %dh%02dm\n' \
      "$name" $(((s - now) / 3600)) $((((s - now) % 3600) / 60))
  done
# monthly-maint starts in 21h49m
```

`--test-only` is the cheap confirmation either way — it reports the start time
the scheduler would actually give the job without queueing anything.

After the fact, a job caught this way says so in `squeue`:

```console
$ squeue -t PENDING -o "%.10i %.12u %.12L %r"
    243393     krausmeg   3-00:00:00 ReqNodeNotAvail, Reserved for maintenance
```

Both of those asked for three days with the window a day out. A job in that
state is not stuck and does not need resubmitting — it is waiting for the
cluster to come back. Shortening `-t` is what makes it run sooner.

`IGNORE_JOBS` means the reservation was allowed to be created over jobs that
were already running, so those are not killed up front — but they do not
survive the window either. **Nothing running is safe across it**, sinteractive
sessions included: a session whose walltime crosses the start time will be cut
short, so launch one that ends before the window rather than one that reaches
past it.

## sinteractive sessions

These are the user's persistent interactive shells. You mostly *observe* them;
create one only when the user wants a durable place to work, not as somewhere
to run a command.

```bash
sinteractive --list --json
# [{"job_id":147845,"name":"agent","state":"RUNNING","node":"compute20",
#   "partition":"rna","cpus":8,"memory":"32G","memory_mb":32768,"gpus":0,
#   "elapsed":"0:43","time_limit":"4:00:00","end_epoch":1783180952,
#   "remaining_seconds":28757,"cwd":"~/devel/proj"}, ...]
```

Get-or-create is one idempotent call — no need to list, parse, and recover
from a duplicate-name error:

```bash
sinteractive --ensure agent --time=4h -j 8 -m 32G -p rna --json
# {... ,"created":true}    launched it
# {... ,"created":false}   one was already running; this is the same object
```

`--ensure` implies `--detach`, accepts the same launch options as a normal
launch, and passes unrecognized flags through to `sbatch`. A `PENDING` match
counts as existing and is returned with `"state":"PENDING"` rather than waited
on, so poll if you need it ready. Two concurrent `--ensure` calls for the same
name can still both launch.

`--list --json` and `--status --json` return the same shape; `--list`
additionally carries `cwd`, which costs an SSH round-trip per session. The
`cpus`/`memory`/`gpus` fields describe the *session's* allocation — use them
to size a separate allocation, never to size work run in the session.

Notes:

- The job cap is **per partition**, not per user. `interactive` allows only 4
  concurrent jobs. `You already have 4/4 interactive jobs` means that one
  partition is full, not the cluster. Launch the session elsewhere with
  `-p rna` rather than cancelling somebody's session to free a slot.
- Never cancel a session you did not create without asking the user.
- Users can rename a session while it runs (`Ctrl-b $`), so resolve and cache
  sessions by `job_id`, not name.

## Check the time budget before long work

```bash
sinteractive --status JOBID --json   # or NAME; includes remaining_seconds
```

Inside a session, `--status` needs no target, and `sinteractive
--agent-context` prints a briefing on the current session and these rules.

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
--status`; age it exactly with `remaining_seconds - (now - updated_epoch)`.

**Re-check before long work; do not trust a budget you read earlier in the
conversation.** Wall time can change underneath you: the user may shorten a
job with `scontrol update JobId=... TimeLimit=...`, or an administrator may
extend one. A number read an hour ago is not evidence about now. After any
such change, `sinteractive --refresh JOBID` makes the cached file agree
immediately instead of at the next poll:

```bash
sinteractive --refresh JOBID --json   # re-check now; same output as --status
```

Note that on most clusters an ordinary user can only *reduce* a job's
TimeLimit — raising it needs operator privileges, and `scontrol` fails with
`Access/permission denied` otherwise. If the user says they extended a job,
confirm it actually took effect rather than assuming it did.

## Observe or drive an interactive session

Sessions live in tmux on the compute node; the socket and session are both
named `sinteractive-JOBID`. The tmux binary is wherever `SINTERACTIVE_TMUX`
points (`/usr/local/bin/tmux` on Bodhi, `/usr/bin/tmux` on some clusters), so
read it from the environment rather than hardcoding a path.

To read what is on screen (last 100 lines):

```bash
ssh NODE "${SINTERACTIVE_TMUX:-/usr/local/bin/tmux}" -L sinteractive-JOBID \
  capture-pane -pt sinteractive-JOBID -S -100
```

To type into it — this is the user's live shell, so only when asked:

```bash
ssh NODE "${SINTERACTIVE_TMUX:-/usr/local/bin/tmux}" -L sinteractive-JOBID \
  send-keys -t sinteractive-JOBID 'command' Enter
```

## Cleanup

Cancel allocations you created as soon as the work is done — `scancel ID` for
an `salloc`, `sinteractive --cancel JOBID|NAME` for a session. Never cancel a
session you did not create without asking the user.

## Related skills

- `slurm-discovery` — which partitions, accounts and QOS you may actually use.
- `bodhi-storage` — read from `/beevol`, scratch on node-local `/tmp`.
- `bodhi-software` — check `module avail` before building or installing.
- `slurm-batch` — `sbatch`, arrays and dependencies, when it is not one job.
