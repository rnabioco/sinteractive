---
name: slurm-batch
description: Running many jobs with sbatch — job scripts, array jobs and their throttling, dependencies between stages, and using sacct to right-size the next run from what the last one actually used. Use when the work is per-sample or per-file rather than a single command, when building a multi-stage pipeline, or when sizing memory and walltime for a batch.
---

# Many jobs, not one

`srun` and `salloc` (the `bodhi-compute` skill) are for one thing at a time,
attached. A batch of hundreds of samples is a different shape: it is submitted
and left, and its sizing is decided once and then applied hundreds of times —
which is what makes getting the sizing right worth a few minutes up front.

## A job script

```bash
#!/usr/bin/env bash
#SBATCH --job-name=star-align
#SBATCH --comment=star-align
#SBATCH --partition=rna
#SBATCH --account=rbi
#SBATCH --cpus-per-task=8
#SBATCH --mem=32G
#SBATCH --time=4:00:00
#SBATCH --output=logs/%x-%j.out
#SBATCH --error=logs/%x-%j.err
set -euo pipefail

module load STAR/2.7.11b            # sbatch starts from a clean login shell
...
```

Name it in **both** `--job-name` and `--comment`, the same short descriptive
value, for the same reason as `srun` — a shared queue should say what is
running and why.

`--output` directories are not created for you: `mkdir -p logs` first, or the
job dies at startup with nowhere to write.

**`DefMemPerCPU` is 4000 MB here.** Leaving `--mem` off does not mean
"unlimited", it means 4G per CPU — a frequent and confusing cause of a job
being killed for memory it never asked for.

## Arrays

One submission, one task per sample:

```bash
#SBATCH --array=0-499%20            # 500 tasks, at most 20 running at once
sample=$(sed -n "$((SLURM_ARRAY_TASK_ID + 1))p" samples.txt)
```

- **`MaxArraySize` is 1001** on this cluster, so indices run `0-1000` and a
  list longer than that has to be chunked into several submissions.
- **Throttle with `%N`.** Without it, 500 tasks all become eligible at once,
  which fills the partition and pushes everyone else — including your own
  later stages — behind them. `%20` is a courteous default; raise it when the
  partition is idle.
- `%A` is the array job id and `%a` the task id, so
  `--output=logs/%x-%A_%a.out` keeps per-task logs apart. `%j` alone collides.
- `MaxJobCount` is 10000 cluster-wide, and the QOS caps submissions per user
  (`normal` allows 2000 submitted, 500 running). `slurm-discovery` covers
  reading those.

## Dependencies

```bash
a=$(sbatch --parsable align.sh)
b=$(sbatch --parsable --dependency=afterok:$a merge.sh)
sbatch --dependency=afterok:$b report.sh
```

`--parsable` prints the bare job id, which is what makes this chainable.
`afterok` waits for success; `afterany` runs regardless; `singleton` serialises
jobs sharing a name.

This cluster runs with `kill_invalid_depend`, so a dependency that can never
be satisfied — the job it waits on failed — is **killed rather than left
pending forever**. A vanished downstream job usually means an upstream
failure, so check that first with `sacct` rather than resubmitting.

## Right-size from what actually happened

Run one sample, then look at what it used before committing to hundreds:

```bash
sacct -j JOBID --format=JobID,JobName%14,ReqMem,MaxRSS,AllocCPUS,Elapsed,State
```

**`MaxRSS` is reported on the step rows, not the allocation row.** The parent
line is blank and `sacct -X`, which shows allocations only, hides it entirely:

```
237176               bash        8G                  4   00:00:36  COMPLETED
237176.0             bash            631852K         4   00:00:36  COMPLETED   <- here
```

That job asked for 8G and touched 617M. Ask for what the measurement says plus
headroom, not a round number that felt safe — over-requesting memory and
walltime is what makes a queue slow for everybody, since the scheduler must
find a hole big enough for the request rather than the job.

Walltime is the same trade in reverse: too short and the job is killed at the
limit with its output half-written, too long and backfill will not slot it in.
`Elapsed` from the trial run is the number to build on.

## Watching and cleaning up

```bash
squeue --me -o "%.10i %.20j %.10T %.10M %r"   # %r is the reason if pending
scancel JOBID                                  # whole array
scancel JOBID_7                                # one task
scancel --me --name=star-align                 # by name
```

Cancel a batch you have abandoned rather than leaving it to drain the
partition. Never cancel jobs you did not submit.
