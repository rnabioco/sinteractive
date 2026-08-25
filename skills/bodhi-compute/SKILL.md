---
name: bodhi-compute
description: Run compute work on the Bodhi HPC cluster. Use whenever a task involves builds, analyses, pipelines, simulations, or any CPU-, memory-, or GPU-heavy or long-running command — such work must run on a compute node inside a Slurm allocation, never on the login node. Covers launching and reusing sinteractive sessions, running commands in an allocation, checking the time budget, and observing the user's interactive sessions.
---

# Running compute work on Bodhi

You are usually on the login node. The login node is for orchestration only:
editing files, git, `squeue`, and other lightweight commands. Anything that
takes more than about a CPU-minute or a gigabyte of memory must run on a
compute node inside a Slurm allocation.

## Find or create a session

Reuse beats relaunching — check what is already running first:

```bash
sinteractive --list --json
# [{"job_id":147845,"name":"agent-test","node":"compute20","partition":"rna",
#   "elapsed":"0:43","time_limit":"15:00","end_epoch":1783180952,
#   "remaining_seconds":28757,"cwd":"~/devel/proj"}, ...]
```

To create one, launch headless. `--detach` returns once the session is ready
(typically ~10 s); with `--json` the only stdout is a status object:

```bash
sinteractive --detach -n agent --time=4h --json
sinteractive --detach -n agent --time=4h -j 8 -m 32G --json      # more CPU/mem
sinteractive --detach -n agent -p gpu --gpus=1 -m 16G --json     # GPU
```

Notes:

- `--time` accepts shorthand (`30m`, `8h`, `2d`). Request only what the task
  needs; ask the user before requesting more than a day.
- Launching a named session that already exists fails with an error listing
  the running job — treat that as "already running" and reuse it.
- The job cap is **per partition**, not per user. The default `interactive`
  partition allows only 4 concurrent jobs, and it is the smallest partition on
  the cluster (~3 nodes). Hitting `You already have 4/4 interactive jobs` does
  not mean the cluster is full — it means that one partition is.
- On a job-limit error, **launch on another partition** rather than reusing or
  cancelling. `sinteractive` passes unrecognized flags through to `sbatch`, so
  `--partition NAME` just works:

  ```bash
  sinfo -o "%20P %5a %10l %6D %6t %N"          # what exists, and what's idle
  sinteractive --detach -n agent --time=2h -j 8 -m 16G --partition rna --json
  ```

  Pick a partition the work belongs to — `rna` for rnabioco work (6+ nodes,
  usually several idle), `normal` as the general fallback, `bigmem` for
  memory-heavy jobs, `gpu` for GPUs. The launch prints a "you already have N
  running sessions" note and proceeds; that note is not an error.
- Never cancel a session you did not create to free a slot. Reuse via
  `srun --overlap` is acceptable but contends for the other job's CPUs and
  memory — prefer a fresh allocation on an idle partition.

## Run commands in the allocation

```bash
srun --overlap --jobid=JOBID -- bash -lc 'cmd ...'
```

- stdout/stderr stream back and the command's exit code is `srun`'s exit code.
- Use the Bash tool's background mode for long commands; `srun` stays attached
  for the duration.
- Several `srun --overlap` commands can run concurrently in one allocation;
  they share the allocation's CPUs and memory.

## Check the time budget before long work

```bash
sinteractive --status JOBID --json   # or NAME; includes remaining_seconds
```

Inside a session, `SINTERACTIVE_JOB_ID` (and `SINTERACTIVE_NAME`) are set and
`sinteractive --status` needs no target. Users can rename a session while it
runs (`Ctrl-b $`), so resolve and cache sessions by `job_id`, not name.

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

Do not start work that cannot finish in the remaining walltime — launch a
fresh session with a longer `--time` (or ask the user to extend the job).

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
named `sinteractive-JOBID`. To read what is on screen (last 100 lines):

```bash
ssh NODE /usr/local/bin/tmux -L sinteractive-JOBID \
  capture-pane -pt sinteractive-JOBID -S -100
```

To type into it — this is the user's live shell, so only when asked:

```bash
ssh NODE /usr/local/bin/tmux -L sinteractive-JOBID \
  send-keys -t sinteractive-JOBID 'command' Enter
```

## Cleanup

Cancel sessions you created when the work is done: `sinteractive --cancel
JOBID|NAME` (or `scancel JOBID`). Never cancel a session you did not create
without asking the user.
