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
#   "elapsed":"0:43","time_limit":"15:00","cwd":"~/devel/proj"}, ...]
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
- The default `interactive` partition caps concurrent jobs per user. If a
  launch fails with a job-limit error, reuse an existing session or ask the
  user which one to cancel — never pick one to cancel yourself.

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
`sinteractive --status` needs no target. For frequent polling, read the state
file instead of hitting the scheduler — it is refreshed about every 30 s:

```bash
cat ~/.cache/sinteractive/JOBID.json
# {"job_id":147845,"name":"agent","node":"compute20",
#  "end_epoch":1783180952,"remaining_seconds":869,"updated_epoch":1783180083}
```

If `updated_epoch` is more than ~2 minutes old, treat the file as stale and
fall back to `sinteractive --status`. Do not start work that cannot finish in
the remaining walltime — launch a fresh session with a longer `--time` (or ask
the user to extend the job).

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

Cancel sessions you created when the work is done: `scancel JOBID`. Never
cancel a session you did not create without asking the user.
