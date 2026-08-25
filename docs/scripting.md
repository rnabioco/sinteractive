# Scripting and agent use

`sinteractive` has a headless mode designed for scripts and coding agents such
as [Claude Code](https://code.claude.com/docs/):

```bash
# Launch without attaching; returns once the session is ready
sinteractive --detach -n mywork --time=8h

# Machine-readable session info
sinteractive --list --json
sinteractive --status mywork --json

# Re-check the budget against Slurm now (e.g. right after a scontrol change)
sinteractive --refresh mywork --json
# {"job_id":147845,"name":"mywork","state":"RUNNING","node":"compute20",
#  "partition":"rna","time_limit":"8:00:00","elapsed":"0:43",
#  "end_epoch":1783180952,"remaining_seconds":28757}

# Run a command inside the allocation (exit code propagates)
srun --overlap --jobid=JOBID -- bash -lc 'make test'
```

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

## Claude Code skill

This repo ships a [Claude Code skill](https://code.claude.com/docs/en/skills)
that teaches agents cluster etiquette: run heavy work in an allocation (never
on the login node), reuse sessions, check the time budget before long jobs,
and clean up. Install it per-user from a checkout of this repo:

```bash
make skill-install   # copies to ~/.claude/skills/bodhi-compute
```
