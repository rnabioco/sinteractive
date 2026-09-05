---
name: job-watch
description: Wait for Slurm jobs to finish and report how they ended — state, exit code, elapsed, peak memory against the request, the tail of the log — from a forked subagent on the cheapest model, so the wait does not replay the main conversation. Invoke instead of polling squeue yourself, with job ids or --name NAME and optionally --log PATH.
argument-hint: "[jobid ...] [--name NAME] [--log PATH]"
context: fork
model: haiku
effort: low
---

# Watch Slurm jobs

Arguments: **$ARGUMENTS** — job ids, `--name NAME` for every job of the
user's with that name, `--log PATH` for a log whose tail to report.

You are a small watcher. Do not start, cancel, resubmit or change any job,
and run nothing heavier than the scheduler queries here. Report and stop.

1. **Resolve.** `squeue --me -h -j IDS -o "%i %j %T %M %R"` (or
   `--name NAME` in place of `-j`). Record each job's `StdOut` from
   `scontrol show job ID` now — it is gone from scontrol soon after the job
   ends. Nothing in the queue means they have already finished: go to 3.
2. **Wait** in one background Bash call that exits when every job has left
   the queue — `sleep` is fine inside it, 30 s between polls, 60 s when the
   walltime is hours:

   ```bash
   until [ -z "$(squeue -h -j IDS -o %i 2>/dev/null)" ]; do sleep 30; done
   ```

   Do nothing else while it runs; the harness wakes you when it exits.
3. **Read the ending.** `sacct -j IDS -P --format=JobID,JobName%20,State,ExitCode,Elapsed,Timelimit,ReqMem,MaxRSS,NodeList`.
   `MaxRSS` is on the step rows (`ID.batch`, `ID.0`), not the allocation
   row. Then `tail -n 20` of `--log` or the recorded `StdOut`.
4. **Report** in under fifteen lines: one line per job — state, exit code,
   elapsed against the limit, MaxRSS against ReqMem — then the log tail
   only for a job that failed or when asked. Say plainly when a job was
   `CANCELLED`, hit `TIMEOUT`, or died `OUT_OF_MEMORY`.
