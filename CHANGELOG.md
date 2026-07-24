# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/rnabioco/sinteractive/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/rnabioco/sinteractive/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rnabioco/sinteractive/releases/tag/v0.1.0
