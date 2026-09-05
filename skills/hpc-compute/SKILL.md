---
name: hpc-compute
description: Run compute on the Bodhi and Alpine (CU Boulder/CURC) clusters. Anything CPU-, memory- or GPU-heavy or long-running — builds, analyses, pipelines, simulations — goes in its own Slurm allocation, never on the login node or in an sinteractive session. Covers telling the clusters apart, srun and salloc, naming jobs, the /tmp boundary, maintenance reservations, sessions, and what to hand to a cheaper model.
---

# Running compute work

## First: where am I?

Two clusters share this skill. Detect which, and read **only that cluster's
file** in this skill's directory for partitions, QOS and local hazards:

```bash
[ -d /scratch/alpine ] && echo alpine || { [ -d /beevol ] && echo bodhi; }
```

- **Alpine** (CU Boulder / CURC) → read `alpine.md` next to this SKILL.md
- **Bodhi** → read `bodhi.md` next to this SKILL.md

`$SINTERACTIVE_JOB_ID` set → you are inside an sinteractive session on a
compute node; unset → the login node. **Either way that shell is for
orchestration, not compute.** Editing, git, `squeue`/`sinfo` and other
sub-CPU-minute commands belong there; everything heavier gets its own
allocation, sized for the job. A session is a small allocation shared with
the shell the user is typing in — never run heavy commands in one and never
`srun --overlap` into one.

## Run work in its own allocation

`SLURM_*` is stripped from a session, so `srun` and `salloc` run from inside
one create their own allocations rather than steps of the session's job.
Both stream stdout/stderr back and propagate the command's exit code.

**Name every job in both fields** — `-J NAME` and `--comment=NAME`, the same
short description of the work (`bwa-align`, not `bash`) — so a shared queue
says what is running and why (`squeue --me -o "%.10i %.20j %.20P %.10M %k"`
shows both). Bodhi's accounting drops the comment, so the name is what
survives in `sacct`; that is why both are filled.

**One-off** — blocks until it finishes (partition and QOS from the cluster
file):

```bash
srun -p PART [--qos=QOS] -c 8 --mem 32G -t 1:00:00 -J make-test --comment=make-test -- make test
```

**Sustained or iterative** — hold one allocation and reuse it:

```bash
ID=$(salloc --no-shell -p PART [--qos=QOS] -c 32 --mem 96G -t 4:00:00 \
       -J cargo-ci --comment=cargo-ci 2>&1 |
     sed -n 's/.*Granted job allocation \([0-9]*\).*/\1/p')   # announced on stderr
srun --overlap --jobid=$ID -- cargo build --release
srun --overlap --jobid=$ID -- cargo test
scancel $ID                                                   # always, when done
```

Naming the `salloc` covers every `srun --overlap` step inside it. Work in
an allocation is bounded by its own `-t`, not the session's walltime — but
the session dying still ends any `srun` you are streaming from, so run long
ones in the Bash tool's background mode.

Request only what the task needs, and ask before requesting more than a day
of walltime or a whole node. Partitions restrict which accounts and QOS may
submit, so the right `-p` can still be rejected — `slurm-discovery` covers
mapping that out and reading why a job is refused or stuck `PENDING`.

### Nothing on `/tmp` crosses into an allocation

`/tmp` is the node's own disk, and `$TMPDIR` and the agent scratchpad are
under it: a script staged there is `No such file or directory` on the node
an `srun` lands on, and a job's `/tmp` output is invisible from the session.
Whatever crosses the session/job line — a script the job runs, its inputs,
output wanted back — goes on the shared filesystem under a directory named
for the task: `~/scratch/<topic>/` on Bodhi, `/scratch/alpine/$USER/<topic>/`
on Alpine. `hpc-storage` has the full pattern.

### A workflow controller is a job, not a session

`snakemake`, `nextflow` and their kind sit for hours submitting jobs. Run in
the session, or in an `srun` held open from it, the controller dies with the
session and the rest of the pipeline with it. Submit it with `sbatch` as a
job of its own and have it drive the real work as Slurm jobs — `slurm-batch`
has the script.

### Check for reservations before asking for walltime

**A job asking for more walltime than remains before a maintenance
reservation does not fail — it is silently deferred to after the window.**

```bash
scontrol show reservation        # ACTIVE = on now; INACTIVE = scheduled
srun --test-only -p PART -c 4 --mem 8G -t 21:00:00 -- true
# srun: Job ... to start at <TIME>   <- the scheduler's real verdict, nothing queued
```

Size `-t` to fit in the gap; if the work cannot fit, say so and let the user
choose between splitting it and waiting. A pending job showing
`ReqNodeNotAvail, Reserved for maintenance` is not stuck — shortening `-t`
is what makes it run sooner. Nothing running survives an all-node window,
sessions included. Bodhi has a recurring monthly one; `bodhi.md` covers it.

## The wait is not your job

Once work is submitted, waiting for it is trivial and should not run on the
model doing the thinking: every poll from the main conversation replays
everything said so far. Fork it — `/job-watch JOBID` waits on the cheapest
model and reports state, exit code, elapsed and peak memory when the job
ends. For an sinteractive session, the MCP server's `wait_for_event` blocks
until something happens; call it from a cheap subagent for the same reason.

## sinteractive sessions

The user's persistent shells. Observe them; create one only when the user
wants a durable place to work, never as somewhere to run a command. With
`sinteractive claude install` the MCP server exposes them as tools
(`list_sessions`, `ensure_session`, `session_status`, `wait_for_event`,
`peek`, `send`) and briefs you at session start; from a plain shell:

```bash
sinteractive list --json                                         # every session; cwd costs an ssh each
sinteractive session ensure agent --time=4h -j 8 -m 32G --json   # get-or-create, idempotent
sinteractive status JOBID --json                                 # remaining_seconds; --refresh re-reads Slurm
sinteractive session peek JOBID|NAME [-n 100]                    # the screen
sinteractive session send JOBID|NAME 'command'                   # the user's live shell — only when asked
```

- The `cpus`/`memory`/`gpus` fields describe the *session's* allocation —
  use them to size a separate one, never to size work run in the session.
- Job caps are per partition or QOS, not per user: `you already have N/N
  jobs` means that one is full — launch elsewhere rather than cancelling
  somebody's session.
- Never cancel a session you did not create without asking. Resolve
  sessions by `job_id`, not name — names can change while a session runs.
- **Re-check the walltime before long work** with `sinteractive status`; a
  budget read earlier is not evidence about now, since a job's TimeLimit can
  be changed underneath you (users can only shorten one; confirm a claimed
  extension actually took effect).

## Cleanup

Cancel allocations you created as soon as the work is done — `scancel ID`
for an `salloc`, `sinteractive cancel JOBID|NAME` for a session.

## Related skills

- `slurm-discovery` — which partitions, accounts and QOS you may actually use.
- `slurm-batch` — `sbatch`, arrays, dependencies and controllers, when it is not one job.
- `hpc-storage` — where work runs and output lands, per cluster.
- `hpc-software` — check the module tree before building or installing.
- `job-watch` — the wait, on a cheap model.
