# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/rnabioco/sinteractive/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/rnabioco/sinteractive/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/rnabioco/sinteractive/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/rnabioco/sinteractive/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rnabioco/sinteractive/releases/tag/v0.1.0
