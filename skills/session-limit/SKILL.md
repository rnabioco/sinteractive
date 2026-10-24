---
name: session-limit
description: Raise (or lower) the time limit of the current sinteractive session, or another job, with scontrol — "/session-limit +2d" adds two days. Checks the partition's MaxTime first and reports whether the cluster let a user do it. Use when a session is running out of walltime.
argument-hint: "+2d | +90m | 1-12:00:00 [jobid]"
context: fork
model: haiku
effort: low
---

# Change a session's time limit

Arguments: **$ARGUMENTS** — a change and optionally a job id. A change is
`+2d`, `+90m`, `+1d12h` (add to the current limit), `-1h` (subtract), or an
absolute limit (`2d`, `1-12:00:00`). The job defaults to the current
session, `$SINTERACTIVE_JOB_ID`.

You change one number on the user's own job and report it. Do nothing else.

1. **Read.** `scontrol show job ID` for `TimeLimit`, `EndTime`, `Partition`,
   then `scontrol show partition PART` for `MaxTime`. The job must be the
   user's own.
2. **Convert** the change to Slurm form: `+2d` is `TimeLimit=+2-00:00:00`,
   `+90m` is `TimeLimit=+01:30:00`, an absolute value is passed as given.
   Work out the resulting limit yourself.
3. **Check the ceiling.** If the new limit exceeds the partition's
   `MaxTime`, stop and say so, with the largest amount that would fit.
   Do not try to get around it.
4. **Apply.** `scontrol update JobId=ID TimeLimit=…`. Slurm on some clusters
   lets only administrators *raise* a limit; if it refuses
   ("Access/permission denied", "Job's time limit can only be reduced"), say
   that plainly. Do not retry, and do not suggest workarounds. The honest
   answer is that the user must ask the cluster admins, or launch a fresh
   session with `sinteractive -t …`.
5. **Verify** with `scontrol show job ID` and report old limit, new limit
   and new end time in one or two lines. The status line and the walltime
   warnings catch up within a refresh or two.

Lowering a limit is always allowed by Slurm; confirm with the user before
lowering a session's limit below its elapsed time, since that ends it.
