# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A `slurm-discovery` skill, for finding out what the cluster actually offers
  instead of assuming it: what the partitions are and how big, which accounts
  and QOS the user holds, and the rule that decides whether a combination is
  submittable — your account in the partition's `AllowAccounts`, and the QOS
  you ask for in both its `AllowQos` and your own association. That
  intersection is the part nobody guesses right: on Bodhi the `gpu` partition
  takes `gpu_rbi`/`gpu_devbio`/`gpu_scb` and not the default `rbi` account, so
  the request is refused however many GPUs are idle, and the error names
  neither half. It also covers reading the QOS limit columns, and `squeue`'s
  reason column when a job is rejected or sits `PENDING`.

  The survey's answers are cached to
  `~/.cache/sinteractive/slurm-map-<cluster>.md` and re-read rather than
  re-run. Keyed by `ClusterName` because one `$HOME` is often mounted on
  several clusters, and a map from the wrong one is worse than none. Only the
  structure is cached — node states and queue depth are re-read live every
  time, so the cached `sinfo` deliberately drops the state column.

- A second Claude Code skill, `git-workflow`, installed alongside
  `bodhi-compute` by `--install-claude`. Where `bodhi-compute` is about the
  cluster, this one is about the repository open in the session: semantic
  versioning with annotated `vX.Y.Z` tags, Conventional Commit messages, one
  worktree per branch under `.claude/worktrees/`, landing work through a pull
  request rather than committing to `main`, and running the repo's own CI
  gates before pushing. It is deliberately general — it names no project, and
  defers to a repository that documents something stricter of its own.

  Sessions are where this work actually happens, and an agent that starts with
  no standing guidance re-derives the conventions every time, or guesses. The
  skill rides the rails `bodhi-compute` already established, so it reaches
  every session on every node with no per-session setup.

### Changed

- The installer no longer names the skills it ships. `--install-claude` copies
  every `skills/*/SKILL.md` found beside the script, and `make install`,
  `make install-system` and `make nodes` install the whole `skills/` tree
  rather than one path each. Adding a skill is now dropping a directory into
  `skills/`, with no install target, fan-out recipe, or copy loop to update in
  step — the previous shape hardcoded `skills/bodhi-compute` in six places,
  and any one of them missed would have shipped a partial set to the nodes.

  `make nodes-check` reports assets present based on the `skills/` directory
  rather than `skills/bodhi-compute`, so it stays true as the set grows.

## [0.3.0] - 2026-08-26

### Changed

- `--install-claude` now registers the hooks for you instead of printing a
  `settings.json` block and leaving the merge to you. The merge is done with
  `jq` and only with `jq` — rewriting the user's settings with string surgery
  in bash could silently disable every setting in the file. It is additive
  (appended to whatever `.hooks` already holds, other keys and their order
  untouched), idempotent (a hook already registered in `settings.json` or
  `settings.local.json` is skipped, matched by script name so a hand-edited
  path or a dropped `bash ` prefix still counts, and a half-registered pair
  gets only its missing half), and it writes nothing when the result would be
  unchanged. The file it replaces is kept as `settings.json.bak-STAMP`, a
  symlinked `settings.json` is resolved first so a dotfiles repo gets its
  target edited rather than its link replaced, and one that does not parse is
  reported and left alone.

  Without `jq` on `$PATH` the block is printed to merge by hand exactly as
  before, with a note that installing one (`pixi global install jq`) lets
  sinteractive do it. `jq` stays an optional dependency.

### Fixed

- `make nodes` now installs the same set of files as `make install-system`,
  the Claude Code assets included; it had shipped only the script, man page
  and completion. The assets belong on the compute nodes because
  `--install-claude` resolves them relative to the running script: someone
  following the status-bar hint runs it from inside a session, which runs the
  node's `/usr/local/bin` copy, and with no `<prefix>/share/sinteractive`
  beside it that call failed. It worked only for people who had also
  installed into a shared `$HOME`.
- `make nodes` renames the script into place instead of writing over it, for
  the reason `tmux-push` does: `--attach` SSHes into a node and runs the
  script there, so a copy can be executing while you install.
- `make nodes` asks pdsh for the ssh rcmd module by name (`-R ssh`, override
  with `PDSH_RCMD`). pdsh defaults to `rsh`, so on a cluster with nothing on
  port 514 the target failed with `connect: Connection refused` for every
  node — and exporting `PDSH_RCMD_TYPE=ssh` did not help under `sudo make`,
  which resets the environment.

### Added

- `make nodes-check` reports the sinteractive version, whether the Claude
  Code assets are present, and the tmux version on every node in `NODES`.
  Read-only and unprivileged. Drift is otherwise invisible — `sbatch` spools
  the submitted copy of the script, so sessions keep working from whatever
  the submitting node has, and a stale compute node only shows up in
  `--attach` and `--install-claude`.

## [0.2.2] - 2026-08-26

### Changed

- Slurm jobs are now asked to carry a name in *both* fields: `-J NAME` and
  `--comment=NAME`, the same short descriptive value. The `bodhi-compute`
  skill, the `--agent-context` briefing, the man page and the docs all show
  it on every `srun`/`salloc` example, so a shared partition stops filling
  with jobs called `bash`. Both fields earn their place because they survive
  differently: the comment is readable on a live job (`squeue`, `scontrol
  show job ID`) but only reaches accounting on clusters that set
  `AccountingStoreFlags=job_comment` — Bodhi does not, so `sacct` history
  keeps the name alone. Name and comment belong to the allocation, so naming
  an `salloc` covers every `srun --overlap` step run inside it.

## [0.2.1] - 2026-08-25

### Fixed

- `pixi.toml` now carries the release version. The docs-site workspace
  had been left at 0.1.0 since the first tag, four releases behind; it
  is bumped alongside the script and the man page from now on, and a
  comment beside it says so.

## [0.2.0] - 2026-08-25

### Changed

- `make tmux` now builds tmux 3.7c (was 3.7b). It fixes the initial state
  of scrollbars so they appear on new windows, which affects sessions
  started with `--mouse`; the rest are a macOS build fix, a redraw-loop
  timing change, a `message-format` default restored to `message-style`,
  and an unzoom-before-floating-pane crash fix. Rebuild and fan out with
  `make tmux && make tmux-push` as root — running sessions keep the tmux
  server they started with until they end.
- The bundled `bodhi-compute` skill, the man page, and the docs no longer
  present an sinteractive session as somewhere to run work. A session is an
  orchestration shell: it defaults to the `interactive` partition, the
  smallest on the cluster, and work run in it competes with the shell the
  user is typing in. Heavy work belongs in its own allocation — `srun` for a
  one-off, `salloc --no-shell` plus `srun --overlap` for sustained work — and
  the guidance now says so, including that `srun`/`salloc` from inside a
  session create their own allocations because `SLURM_*` is stripped. The
  previous advice to `srun --overlap` into a session has been retired, and
  the `--detach` banner no longer prints it.

### Added

- `--install-claude` installs the Claude Code skill and hooks into
  `~/.claude` from any installed copy of sinteractive, not just from a git
  checkout. `make install` now ships the assets beside the script under
  `<prefix>/share/sinteractive`, and the flag finds them relative to the
  running script — so on a cluster where an admin installed sinteractive with
  `make nodes`, users can pick up the integration without cloning anything.
  `SINTERACTIVE_SHARE` overrides the lookup, and `make claude-install` is now
  a thin wrapper around the same code path rather than a second copy of it.
- The yellow rule between the pane and the status bar carries a centred,
  scrolling `sinteractive --install-claude` notice while Claude Code is
  running in a session whose hooks are not yet registered. The rule is full
  width and otherwise empty, so this costs nothing in the status line, which
  is already shared by the job info and the Help/Detach keys. The notice
  scrolls through a fixed 44-column window at about three columns a second,
  held constant across the countdown's phases so the final 10Hz countdown
  stays fluid; the loop only wakes at that rate while a notice is showing. The hint is gated on a
  live `claude` process rather than on an installed binary, and clears once
  the hooks appear in `settings.json`, so it never nags anyone who does not
  use Claude Code.
- The in-session help popup (`Ctrl-b h`) now reports the sinteractive
  version. It comes from Slurm's spooled copy of the script, so it is the
  version that launched the session, which can differ from what is installed
  on the login node now.
- `--agent-context` prints a briefing, for a coding agent running inside a
  session, on which job it is in, how big that allocation is, how much wall
  time is left, and the rule that a session is an orchestration shell rather
  than a compute target — with the `srun` and `salloc` command shapes to use
  instead. Exits 1 outside a session. Run it by hand to see exactly what an
  agent was told.
- `--ensure NAME` is an idempotent get-or-create: reuse the session named
  `NAME` if one is running, otherwise launch it. Implies `--detach` and takes
  the usual launch options; `--json` adds a `created` field. Replaces the
  list-parse-launch-recover-from-duplicate-name dance callers used to need.
- Two Claude Code hooks, in `claude/hooks/`, for agents that run **inside** a
  session: a `SessionStart` hook that emits `--agent-context`, and a
  `UserPromptSubmit` hook that stays silent until the session drops below
  `SINTERACTIVE_AGENT_WARN` seconds remaining (default 1800) and then warns
  that long work will not survive it. Both always exit 0, so they are
  harmless on the login node and in unrelated projects.
- `make claude-install` installs the skill and both hooks into `~/.claude`
  and prints the `settings.json` block to merge (it does not edit the file).
  `make skill-install` is now an alias for it.
- `--status --json` and `--list --json` now report `cpus`, `memory`,
  `memory_mb` and `gpus`. These describe the session's own allocation and are
  meant for sizing a *separate* allocation to run work in; they are
  deliberately not exported into the session environment, where a
  `SINTERACTIVE_CPUS` would invite running `make -j` in the wrong place.
- `--list --json` now includes `state`, so it returns the same object shape
  as `--status --json` except for `cwd`.
- `--refresh [TARGET]` re-checks a session's time budget against Slurm and
  makes its cached state file agree now rather than at the next poll — one
  command for "I just changed this job's wall time with `scontrol`". Output
  is identical to `--status`, including `--json`.
- `--list --json` now reports `end_epoch` and `remaining_seconds` per
  session, so one call can rank sessions by remaining wall time instead of
  a `--status` per job.
- `SINTERACTIVE_POLL` sets how often a session re-checks its end time and
  rewrites its state file (default 30 seconds).

### Fixed

- The state file (`~/.cache/sinteractive/JOBID.json`) could advertise a
  fresh `updated_epoch` over a `remaining_seconds` derived from an end time
  up to five minutes old, because the end time was re-queried every 300
  seconds while the file was rewritten every 30. A wall-time change made
  with `scontrol update JobId=... TimeLimit=...` was therefore invisible to
  scripts and agents — and looked authoritative while it was wrong, since
  the documented "older than ~2 minutes means stale" check could never fire.
  The end time is now confirmed against Slurm immediately before every
  write, so `updated_epoch` means "confirmed against Slurm at this time".
- When `squeue` cannot be reached the state file is now left untouched
  rather than restamped with an unverified budget, so it ages honestly and
  the documented staleness check detects it. The status-bar countdown keeps
  running from the last known end time either way.
- The state file is refreshed every 30 seconds as documented. The status
  loop's one-minute sleep in the green phase could satisfy the 30-second
  write gate only once, making the real cadence 60 seconds.
- The terminal bell and the red final countdown no longer fire off a stale
  end time: the deadline is confirmed before the bell rings, the same
  re-check the clean-shutdown path already did. Previously a wall-time
  change could ring the bell and start the red spinner on a session that
  had minutes or hours left.

## [0.1.3] - 2026-07-24

### Added

- `--attach` with no target reattaches to your only running session. With
  several running it lists them with ready-to-run `--attach` commands to
  pick from; with none it says how to start one.
- `--cancel JOBID|NAME` cancels a session and reports what it cancelled.
  Unlike `scancel` it accepts session names, so the whole lifecycle
  (`--attach`, `--status`, `--cancel`) works by name.
- Bash completion for options and, after `--attach`/`--status`/`--cancel`,
  the job ids and names of running sessions. Targets are read from the
  state files in `~/.cache/sinteractive/` rather than `squeue`, so
  completion stays instant when the scheduler is slow. Installed by
  `make install` (and by `make nodes` for system-wide deployments).

### Changed

- The pending-job wait now reports why the job is waiting (Slurm's pend
  reason) and its estimated start time, on a spinner line that updates in
  place, instead of printing a bare dot every five seconds. Redirected
  output keeps the dot-per-poll trail for logs.
- Interrupting a launch with `Ctrl-C` while the job is pending now
  confirms the cancellation ("Cancelled job 12345.") instead of exiting
  silently.
- Launching while sessions are already running prints a note naming a
  running session and how to reattach, then proceeds — aimed at the
  forgotten-detach case. The job-limit check is still what refuses.
- The `--detach` summary now suggests `sinteractive --cancel NAME` rather
  than a raw `scancel JOBID`.

### Fixed

- A cancelled launch no longer runs `scancel` twice: the cleanup handler
  called `exit`, which re-entered it through the `EXIT` trap. Previously
  silent (`scancel --quiet`), this surfaced as a duplicated cancellation
  message.
- The README now carries the keyboard copy/paste key table added in
  0.1.1, which had only reached the `Ctrl-b h` popup and the docs site.
  The GitHub landing page kept a stale one-line "Scroll up" row and never
  mentioned that copied text reaches the local clipboard over OSC 52.

## [0.1.2] - 2026-07-24

### Fixed

- The session tables shown after detaching (`Other running sinteractive
  sessions` / `You have other sinteractive sessions still running`) no
  longer misalign columns when a session name is long. The fixed-width
  `squeue --Format` output let a comment of 30+ characters run into the
  node field with no separator, shifting every column left; the tables
  now use pipe-delimited `-o` output like `--list` already did.

## [0.1.1] - 2026-07-24

### Changed

- Copy mode is now always vi-keys (`mode-keys vi`), regardless of
  `$EDITOR`, so keyboard selection works the same for everyone: `Ctrl+b
  [`, `Space` to select, `Enter` to copy (into the local clipboard via
  OSC 52) — a smoother alternative to jumpy mouse/scrollbar selection.
- The `Ctrl+b h` help popup gained a "Scrollback & copy" section with
  the copy-mode keys.

## [0.1.0] - 2026-07-24

First tagged release.

### Added

- Persistent interactive sessions: a batch job starts a detached tmux
  session on the allocated compute node, then SSHes in and attaches.
  Sessions survive SSH drops; reconnect with `--attach JOBID|NAME`.
- Named sessions with `-n`/`--name`; rename a running session in place
  with `Ctrl+b $`.
- `--list` shows running sessions (including each session's working
  directory); `--status` reports on one session. Both support `--json`
  output for scripts and agents.
- Headless `--detach` mode for automation, plus a state file mirroring
  the time budget (`~/.cache/sinteractive/<jobid>.json`).
- Time-limit awareness: a status-bar countdown with yellow and red
  warning phases, a terminal bell as the final countdown starts, and a
  clean self-shutdown just before the walltime limit.
- Refusal to submit jobs whose `--time` would overlap an upcoming MAINT
  reservation, with a suggested shorter duration.
- Interactive-partition job-limit check with a listing of current
  sessions and reattach/cancel commands.
- Time shorthand (`8h`, `30m`, `1d12h`, ...) converted to Slurm's
  `[D-]HH:MM:SS` format, with carry normalization (`90m` → `01:30:00`).
- Pass-through of unrecognized options to `sbatch` (e.g. `--partition`,
  `--gres`).
- Environment-variable defaults: `SINTERACTIVE_TIME`, `_PARTITION`,
  `_QOS`, `_CPUS`, `_MEM`, `_TMUX`, `_MOUSE`, `_WARN_YELLOW`,
  `_WARN_RED`.
- In-session help popup (`Ctrl+b h`) with live job info; opt-in mouse
  support (`--mouse`) with scrollbars and OSC 52 clipboard integration.
- `-V`/`--version` flag; the script's `VERSION` constant is kept in
  sync with the release tag.
- Friendly errors: targeted hints for combined short flags (e.g.
  `-la`), and clean reporting when `sbatch` rejects a submission.
- Man page, zensical docs site with GitHub Pages deploy, Makefile
  installers (user, system-wide, and per-node fan-out), and a
  `bodhi-compute` Claude Code skill.

[Unreleased]: https://github.com/rnabioco/sinteractive/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/rnabioco/sinteractive/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/rnabioco/sinteractive/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/rnabioco/sinteractive/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/rnabioco/sinteractive/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/rnabioco/sinteractive/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/rnabioco/sinteractive/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/rnabioco/sinteractive/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rnabioco/sinteractive/releases/tag/v0.1.0
