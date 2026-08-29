# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `wait_for_event` no longer loses an event whose line was half-written when
  the call began. It took the file's length as its starting point, and the
  sampler appends a line in more than one write now and then, so a call that
  started between two of them read the line's second half alone, failed to
  parse it and waited on for something else — the intermittent CI failure of
  `partial_lines_wait_for_their_newline`, and a real miss for an agent whose
  wait began at the wrong moment. The starting point is now the beginning of
  any unterminated last line.

- `sinteractive claude install` writes the absolute path of the binary that
  ran it — for the MCP server, both hooks and the statusline — rather than
  the bare `sinteractive`. Claude Code starts all four from a non-interactive
  shell, where an alias does not apply and PATH order does, so on a machine
  with the 0.x script still in `/usr/local/bin` ahead of `~/.local/bin` the
  MCP server was launched as `/usr/local/bin/sinteractive claude mcp`, which
  took `claude mcp` for a job command and died in `sbatch`; the session then
  had no `mcp__sinteractive__*` tools at all. Entries an earlier install
  wrote are pointed at the new binary on the next install instead of being
  left alone, so re-running it after an upgrade heals a configuration that
  already has the bare name (an MCP entry's `env` and the user's own hooks
  are kept as they are).
- `make install` no longer kills the sessions that are running. The binary
  was `install`ed over `~/.local/bin/sinteractive` in place, and on an NFS
  home (Alpine) that pulls the inode out from under every process executing
  it on another node — each session's zellij server, and the `launch` or
  `attach` on the login node — which then die with SIGBUS the next time
  they touch a page not yet in memory: "Lost connection to the Zellij
  server" on exit, followed by "Bus error (core dumped)" from the login
  shell. Each build now lands as `<bindir>/.sinteractive-<sha>` with
  `sinteractive` a symlink swapped in by rename, an existing in-place copy
  is hard-linked to a versioned name first so its inode survives the swap,
  and old builds are pruned only when no queued or running job can be using
  them (everything since the earliest submit time in the queue stays, plus
  two before it). A session also keeps spawning its helpers from the build
  it started on, since `current_exe()` resolves through the symlink.
- The `Ctrl+b q` popup says what it is and how to leave. `queue --watch`
  slept between redraws and read no keys, so the only way out of the floating
  pane was Ctrl-C; worse, the frame was as long as the recent history, so in
  a short pane it scrolled its own heading off the top and left a wall of
  rows with no name on it. The watch view now draws to the pane it is in — a
  title, the tables clipped to what fits (`… N more`), and a key legend
  pinned to the bottom row — and reads keys: `q` or `Esc` quits, `r` redraws
  now, Ctrl-C and Ctrl-D still work. With output redirected it stays the
  plain redraw loop it was.
- The bar's help legend names the key that turns its pages. `(1/2)` said
  there was more to see but not how to reach it; the counter now comes with
  `^b h more · ^b esc close`, dropped first when the bar is too narrow for
  both it and the keys. A single page shows no counter at all.
- The bar's fence rule is a lighter orange (`#E6A692` dark, `#DF8B70` light)
  rather than the full accent. It runs the whole width of the pane, where the
  accent's own weight pulled the eye off the line beneath it; the glyph, job
  ids and gauges keep the accent.
- The terminal-background query no longer leaves its answer at the prompt.
  OSC 11 went out once per palette — two or three times in a `launch` — each
  with a 100 ms window, so on anything but a local terminal the reply landed
  after the window had closed and the shell echoed it as
  `^[]11;rgb:2828/2c2c/3434^[\`. It is now asked once per process, and a
  Device Attributes query rides along behind it: terminals answer in order,
  so its reply is the signal that the colour answer either arrived or never
  will, and the read ends on that rather than on a stopwatch. Inside a zellij
  pane the query is skipped altogether — zellij forwards it to the host
  terminal and gives that a full second, far longer than a CLI may stall.
- Leaving a session no longer ends with `Bye from Zellij!` under a screenful
  of blank lines: zellij's parting message came with a jump to the last row,
  which threw away the cursor position that leaving the alternate screen had
  just restored. sinteractive's own teardown summary is what remains.
- The status bar reads on the dark backgrounds people actually use: the
  secondary grey and the hint blue are brighter (`#CCCCCC`, `#C8CEFF`) and
  `▣ N jobs monitorable` is bold.
- The quota notice sizzles red → orange → yellow instead of red → blue, which
  was easy to miss at a glance.
- `Ctrl+b m` puts you back into the monitor panel after you have stepped out
  of it. The panel names its own pane `sint-monitor` so the bar can find it
  in the pane manifest, but it asked for that name in the same breath as the
  permission to do so — and zellij answers a permission request with an
  event, never inline, so the rename was refused and never retried. A
  nameless panel is invisible to the bar, which then read every later
  `Ctrl+b m` as "no panel yet, open one" and re-applied the panel layout,
  whose shell pane takes the focus — undoing the very move the keypress had
  asked for. The panel now claims its pane once zellij has answered, and
  re-asserts the name if a pane update ever shows it missing.

### Changed

- The monitor panel is bars, and a pane you can step into. It opens six rows
  tall — a strip of every monitorable job, then CPU, memory and a row per
  GPU — and `Ctrl+b m` now *focuses* it instead of toggling it: it opens the
  panel if it is closed, moves the focus into it if it is open, and hands the
  focus back to the shell if the panel already has it. The panel keeps
  running through all of that. Focused, it takes bare keys: `←`/`→` (or
  `h`/`l`) step through jobs, `t` opens the full `sinteractive monitor` TUI
  for the selected job in a floating pane — that is where the process table
  now lives, scrollable and sorted, rather than squeezed into the panel —
  `Esc`/`q` hand the focus back, and `x` closes the panel. Every `Ctrl+b`
  chord keeps working while it is focused, because zellij resolves the prefix
  before the focused pane sees a key. The bar no longer keeps its own tally
  of whether the panel is open; it reads the pane manifest, so closing the
  panel with `Ctrl+b x` leaves nothing stale behind.
- The status bar separates itself from the shell with a heavy accent rule,
  the way 0.x did with tmux's `pane-border-lines heavy`. zellij's panes are
  borderless, so the bar draws the rule as its own first row and stands two
  rows tall; with the monitor panel open the region reads as a framed block —
  rule, panel, rule, line.
- Values in the bar and the panel are no longer dimmed, only their labels
  are. The job id, the host, the load, the GPU figures and the time left
  print at the terminal's own foreground weight, so the numbers carry and the
  words around them recede.
- Key hints name the prefix — `^b n next`, `^b esc back`, `^b ,/. job`,
  `^b m focus`. Ctrl+b is one-shot, and a bare `n` read as though the key
  worked on its own.
- `--help`, usage and argument errors are coloured — headings and usage in
  the accent, flags green, placeholders cyan — and honour
  `SINTERACTIVE_COLOR` like everything else sinteractive prints.
- The notices line drops its key legend before it truncates the notice: on a
  narrow bar the notice itself is the point.

## [1.0.0] - 2026-08-28

sinteractive 1.0 is a rewrite in Rust with [zellij](https://zellij.dev)
compiled in. The bash script and its tmux dependency are gone; what is left
is one binary that is the launcher, the batch job, the zellij server and
client, the status bar, and the agent-facing tooling. The session contract
— the `sinteractive[:NAME]` Comment marker, the `<cache>/JOBID.json` state
file, the `status`/`list` JSON shapes — is unchanged, so anything that
scripted 0.x keeps working.

### Added

- **zellij, compiled in.** The binary carries zellij 0.45.1 as a library and
  is its own server and client, so the compute nodes need no multiplexer and
  nothing has to be fanned out to them: the batch job execs the installed
  binary from wherever it is (a shared filesystem), and the status plugin and
  config are extracted once into the cache directory. `sinteractive zellij …`
  is the full zellij CLI for anyone who wants it.
- **A status plugin** in place of tmux's status strings: one row at the
  bottom of every session with the job id, name, node, walltime remaining
  (yellow, then red, as it runs down), a count of your other jobs, and a
  `⚠ N notices` counter. `Ctrl+b n` reads the notices inline, one at a time;
  `Ctrl+b h` shows the key legend inline. Segments drop from the right as the
  terminal narrows, so the bar is always exactly one row.
- **A monitor panel**, `Ctrl+b m`: a 12-row nvitop-style view between the
  shell and the bar — CPU and memory against the job's cgroup limits, load,
  GPUs, the busiest processes. `Ctrl+b ,` / `.` step through hosts when there
  is more than one.
- **`sinteractive queue [--all] [--watch]`**: your running and pending jobs
  with pend reasons, and the last day's history with a memory right-sizing
  hint. `Ctrl+b q` opens it in a floating pane inside a session.
- **`sinteractive monitor [TARGET|HOST] [--live] [--json]`** and
  **`sinteractive snapshot [--json]`**: the same numbers as the panel from
  the login node, read from the snapshot the session writes to
  `<cache>/JOBID.metrics.json` every few seconds — no ssh — or sampled live
  over ssh from any host.
- **`sinteractive peek TARGET [-n N]`** and **`sinteractive send TARGET
  COMMAND`**: read the last lines of a session's screen, or type into its
  shell, from the login node or an agent. Replaces the `ssh NODE tmux
  capture-pane` recipe the skills used to document.
- **`sinteractive events [TARGET] [--follow] [--since EPOCH]`**: the
  session's event log (`<cache>/JOBID.events.ndjson`) as NDJSON.
- **`sinteractive doctor [--nodes] [--json]`**: is this install able to run
  a session from here — binary, embedded plugin, bundle, cache dir, Slurm
  tools, ssh, NVML, whether `$HOME` is somewhere the cache can live — and,
  with `--nodes`, the same from every compute node over ssh. Replaces
  `make nodes-check`.
- **Native Claude Code integration.** `sinteractive hook session-start` and
  `sinteractive hook prompt` replace the two hook scripts; `sinteractive
  statusline` is a `statusLine` command showing model, context usage and,
  inside a session, the remaining walltime and notice count, from the cache
  files only; `sinteractive mcp` is a Model Context Protocol server over
  stdio with typed tools for every `--json` command plus `wait_for_event`.
  `install-claude` registers all three — hooks, statusline, MCP server — and
  removes the 0.x hook scripts it finds, without needing jq.
- **`sinteractive schema`** dumps the JSON schemas of the machine-readable
  outputs; **`man`** and **`completions bash|zsh|fish`** generate the man
  page and shell completions from the clap definitions, so they cannot drift.
- **`SINTERACTIVE_THEME`** (`dark`/`light`/`auto`) and a Claude Code palette
  shared by the CLI, the status bar, the monitor and the statusline; the
  terminal's background is detected when unset.
- **`SINTERACTIVE_CACHE`** to put the state files and the extracted bundle
  somewhere other than `~/.cache/sinteractive` (Alpine's 2 GB `/home`).
- **A fake-slurm test harness** (`tests/fake-slurm/`): shims for `squeue`,
  `sbatch`, `scontrol`, `scancel`, `sacct`, `sacctmgr`, `sinfo`, `srun` and
  `ssh` so the integration tests exercise launch, attach, ensure, status,
  peek/send, install-claude and the MCP server without a cluster; and a CI
  workflow that lints, tests, and builds the release binary in a
  `rockylinux:8` container so it runs on glibc 2.28.

### Changed

- **Subcommands.** `sinteractive status|list|attach|ensure|cancel|refresh|
  quota|agent-context|install-claude` replace the 0.x top-level flags. A bare
  `sinteractive [OPTIONS] [SBATCH ARGS…]` still launches, and unknown flags
  still go to `sbatch` in any order. The old flags (`--status`, `--list`,
  `-a/--attach`, `--ensure`, `--cancel`, `--refresh`, `--check-quota`,
  `--agent-context`, `--install-claude`) are accepted for this one release
  and warn on stderr.
- **Mouse mode is on by default** (`--no-mouse` / `SINTERACTIVE_MOUSE=off`
  to turn it off), and selecting text copies it to the local clipboard.
- **Attach goes through `srun --overlap --pty`** by default, which needs no
  ssh access to the node; `attach --ssh` keeps the `ssh -X` path for X11.
- **The job is submitted with `sbatch --wrap`** rather than by handing
  `sbatch` the program as its script: a multi-megabyte binary would not fit
  through the controller's `MaxScriptSize`, and this way a session runs the
  installed binary rather than a spooled copy.
- **Deployment model.** One binary on a shared filesystem, glibc-linked and
  built on the oldest glibc it must run on; `make install`/`install-system`
  require the built binary and install the generated man page and
  completions (bash, zsh, fish); `make nodes` only copies the binary and the
  share tree, for a node-local `/usr/local`.
- The hooks, statusline and MCP server are subcommands of the binary, so
  `install-claude` copies only the skills and edits settings.
- The skills are installed from the share tree beside the binary
  (`<prefix>/share/sinteractive`) or the checkout named by
  `SINTERACTIVE_SHARE`; the `claude/hooks` directory in that tree is empty.

### Removed

- The bash implementation (`sinteractive` at the repo root) and everything
  that existed to serve it: the tmux dependency, `SINTERACTIVE_TMUX`, the
  `make tmux`, `tmux-deps`, `tmux-push`, `tmux-all` and `nodes-check` targets,
  the hand-written man page and bash completion, the
  `claude/hooks/sinteractive-session-context.sh` and
  `sinteractive-walltime-guard.sh` scripts, and the Makefile's fallback to
  installing the script when the binary was not built.
- `Ctrl+b $` in-session rename and the terminal bell at the final countdown
  have no zellij equivalent yet; the red bar is the cue.

## [0.7.0] - 2026-08-27

### Changed

- The cluster skills now cover Alpine (CU Boulder / CURC) alongside Bodhi,
  and the `bodhi-*` skills are renamed `hpc-*` to match — `hpc-compute`,
  `hpc-software`, `hpc-storage`. Each of the three is now a short SKILL.md
  that detects the cluster and delegates to an `alpine.md` or `bodhi.md`
  beside it, so an agent reads the shared rules plus the system it is
  actually on and never the other one's partitions, paths, and quotas.

  The two clusters differ where it hurts: Alpine's filesystem is tiered (a
  2 GB `/home` nothing may be written to, a small backed-up `/projects`, a
  huge 90-day-purged `/scratch/alpine` where all work runs) where Bodhi has
  one shared `/beevol`; Alpine requires a QOS on every job and pairs each
  partition with its own QOS family; Alpine runs Lmod (hierarchical,
  `module spider`) where Bodhi runs Tcl modules; and Alpine couples memory
  to CPUs (`MaxMemPerCPU=3840M` on `acpu`). The Alpine files record the
  live-survey commands and the survey date (2026-08-27) so the facts can be
  re-scraped when CURC changes them. The `slurm-batch` and
  `slurm-discovery` skills stay single-file, with their per-cluster numbers
  labelled inline.

  `--install-claude` now copies each skill's whole directory rather than
  SKILL.md alone, and removes a stale `bodhi-*` copy from `~/.claude/skills`
  when it installs the `hpc-*` successor, so pre-rename installs do not end
  up with two skills claiming the same job. The asset probe accepts a
  pre-rename checkout named via `SINTERACTIVE_SHARE`.

- The notice lines below the status bar are gone. Everything the session has
  to say about itself is collapsed into a compact `⚠ N notices` counter at
  the right of the session line — red while the quota warning is among them,
  yellow otherwise, absent when there is nothing to say — so the panel is
  always exactly one status line tall. A new `Ctrl-b n` popup shows the
  notices in full (`Ctrl-b n` overrides tmux's stock next-window, which has
  nothing to do here with the window list hidden), and `sinteractive
  --status` prints the same text from the login node, reading the notices
  file the session maintains in `~/.cache/sinteractive/`.

  While the quota warning holds, the counter shimmers — a lighter band
  sweeping through the red text, the way Claude Code's spinner verbs do —
  so the one notice that needs acting on keeps catching the eye without
  taking any more room than the others.

  The warnings held a row of pane height for their whole life — quota and a
  maintenance-trimmed end time never clear on their own — and every
  appearance resized every pane in the session. A counter does neither, at
  the price of one keypress to read the text; and with the width limit gone,
  the maintenance notice can afford to say what it means again.

- The `Detach: Ctrl+b d` hint is out of the status line; it already lives in
  the `Ctrl-b h` help popup, which now also lists `Ctrl-b n`.

## [0.6.0] - 2026-08-26

### Changed

- The reporting commands are in colour. `--help`, `--list`, `--status`,
  `--check-quota` and `--install-claude` printed flat text while the launch
  and teardown narration beside them was already colourised, so the two
  halves of the same tool did not look like the same tool. Job ids and node
  names are teal wherever they appear, as they already were in the narration
  and on the status bar; labels are yellow; secondary text is dim.

  Colour is decided per stream rather than once at startup. The narration
  writes to stderr and asks about stderr; the reporting commands write to
  stdout and ask about stdout, so `sinteractive --list | less` carries no
  escapes and a plain `sinteractive --list` does. `SINTERACTIVE_COLOR`,
  `NO_COLOR` and `TERM=dumb` work as before, on both.

  Two things now read by colour rather than by parsing: `--status` shades the
  remaining walltime yellow under an hour and red under fifteen minutes, and
  every table that lists jobs marks a `PENDING` one yellow against a green
  `RUNNING`. In the job-limit error that is the point — a pending job holds a
  slot exactly as a running one does.

  Errors are uniform throughout: a red bold label, the message beside it, and
  any follow-on hint dimmed under it, so the thing to read and the thing to do
  next are told apart at a glance. Text sinteractive is quoting rather than
  writing — sbatch's own stderr — is left exactly as sbatch produced it.

- The warnings line under the status bar is split, quota flush left and the
  maintenance-trimmed end time flush right, matching the two ends the session
  line above it already uses. They used to sit side by side at the left with a
  separator between them, which read as one long run of text with the middle
  of the line empty.

  Both are shorter. The quota notice reports the overage instead of the usage
  — `⚠ QUOTA over by 204.8G (30T limit)` — because that is the number you act
  on, and "over by" already says you are over. The maintenance notice drops
  its `SHORT SESSION` label: in yellow, on the warnings line, an end time that
  is not the one you asked for is already reading as a warning, so the space
  goes to the reservation name instead.

## [0.5.1] - 2026-08-26

### Changed

- Everything the session has to say about itself moved below the status bar,
  onto lines of its own. The pane border was carrying three unrelated things
  at once — `OVER QUOTA`, `SHORT SESSION`, and a scrolling offer to install
  the Claude Code hooks — sharing one rule and taking turns for it. The line
  you glance at for the job id was busy enough to stop reading.

  There are now up to two lines under the session line, each present only
  while it has something on it: warnings nearest (the two share a line, since
  both hold for the whole session), then the Claude Code hint, furthest away
  because it is an offer rather than something to act on. The pane border is
  back to being a plain rule with no text. The status bar grows and shrinks
  with the notices, so a session with nothing to say is exactly as tall as it
  was before.

  The hint no longer scrolls. It scrolled because a 44-column window was all
  the border could give it without swallowing the rule; a full-width line of
  its own fits the whole sentence in an 80-column terminal. That also retires
  the 0.3s redraw the marquee forced on every session that showed it.

  One tmux subtlety, found by rendering it rather than by reading: setting one
  element of an option array at session scope replaces the whole array for
  that session instead of overlaying the global one, so a session-scoped
  `status-format[1]` leaves `status-format[0]` empty and the session line —
  the thing the notices are meant to sit under — silently disappears. The
  extra indices are set globally instead, which leaves the default index 0
  alone.

## [0.5.0] - 2026-08-26

### Added

- Colour in the launch and teardown narration. Four roles rather than a
  rainbow: teal for identifiers (job ids, node names), echoing the `#2DBFB8`
  the status bar already uses for the same things so the two agree about what
  an identifier is; dim for progress, which leaves `✓ Session … is ready` the
  only bright line in the block; yellow for keys and warnings; red for errors.

  `SINTERACTIVE_COLOR` takes `auto`/`always`/`never` (default `auto`), and
  `NO_COLOR` is honoured whatever its value. The `auto` test is on **standard
  error**, not standard output, because that is where the narration goes — the
  two differ in exactly the case that matters, since `--detach ... > file`
  should still narrate to the terminal while `2>log` must stay free of
  escapes. With colour off every variable is empty rather than being guarded
  at each use, so one set of format strings serves both and there is no
  second, less-tested path.

  Two messages were tightened in passing: `Interactive job with ID N
  submitted, please wait` is now `Submitted job N, waiting for it to start`,
  and the detach block's prose `To reconnect:` / `To cancel the job:` became an
  aligned `Reconnect:` / `Cancel:` pair matching the one shown at launch.

- Storage quota in the notice line. A session shows a red `OVER QUOTA` warning
  above its status line, with the overage, while the user is past their hard
  limit, re-checked every `SINTERACTIVE_QUOTA_POLL` seconds (default 600).

  Both halves are readable from a compute node, which is what makes this
  possible without a head-node round trip: the hard limit comes from the
  shared quota file, and usage from the quota daemons, which answer
  `QUOTA <uid>` with `OK <kilobytes>` per target. Bodhi's own `quota_check`
  lives only on the head node, so the obvious implementation is
  `ssh head quota_check` — but the daemons listen to the compute nodes
  directly, so a session can just ask. The whole probe is bash and
  `/dev/tcp`, takes about a second, and reproduces `quota_check -b`'s numbers
  exactly.

  The result is cached per user rather than per session, so six open sessions
  still cost one probe per interval. Every input is overridable
  (`SINTERACTIVE_QUOTA_FILE`, `_HOSTS`, `_PORT`, `_TIMEOUT`) and every failure
  is silent, so a cluster without these daemons simply never shows the notice.

- `--check-quota`, which re-checks now, rewrites the shared cache and tells
  every running session to re-read it, so the warning clears within a tick
  instead of up to ten minutes later. This is the command to hand an agent
  that has just deleted something on the user's behalf: leaving a stale
  warning on screen makes it look like the deletion failed. Exits 0 whether or
  not the user is over — being over quota is a fact to report, not a failure
  of the check — and 1 only when the quota cannot be read. `--json` for
  scripting. The `bodhi-storage` skill and `--agent-context` both now tell
  agents to use it.

### Changed

- A session whose walltime would run into a maintenance window is now
  **shortened to fit, rather than refused**. Slurm will not start a job that
  overlaps the reservation — it defers it until the window closes, which can
  be a day or more — so at the default day-long request a session simply stops
  starting as maintenance approaches. The previous behaviour caught that and
  printed the shorter command to run instead, which is correct but hands the
  arithmetic back to the user at exactly the moment they wanted a shell.

  The launch now says what it did, and the session carries a yellow
  `SHORT SESSION` notice for its whole life so the shortened allocation stays
  visible after the launch output has scrolled away:

  ```console
  $ sinteractive -n analysis
  Maintenance (monthly-maint) starts Thu Aug 27 06:00.
  Shortened the request from 24:00:00 to 17:10:43 so the session ends before it.
  ```

  A launch is still refused when under 10 minutes remain before the window,
  since the session would die almost immediately, and an explicit
  `--reservation` is still left alone.

- The notice line is now ranked rather than single-purpose: over-quota (red)
  outranks a short session (yellow), which outranks the scrolling Claude Code
  hint. The first two are static — a marquee is right for an invitation and
  wrong for a warning — and share the line when both apply. A session showing
  a static warning also stops paying for the marquee's 0.3s redraw.

## [0.4.0] - 2026-08-26

### Added

- `bodhi-compute` now covers Bodhi's monthly maintenance reservation, and
  sizing walltime around it. The reservation carries `ALL_NODES`, so there is
  nowhere on the cluster to run during the window, and the failure mode is
  quiet: a job asking for more walltime than remains before the start is not
  rejected, it is **deferred to after the window**. Measured against a window
  21h49m out, `-t 21:00:00` started immediately and `-t 22:00:00` was pushed
  to the reservation's end time — one extra hour of request bought two days of
  waiting. The section covers reading `scontrol show reservation`, computing
  the gap, confirming with `srun --test-only`, and recognising
  `ReqNodeNotAvail, Reserved for maintenance` in `squeue` as "waiting for the
  cluster to come back", not something to resubmit. `IGNORE_JOBS` means jobs
  already running are not killed when the reservation is created, but nothing
  survives the window itself — sinteractive sessions included, so a session
  should be launched to end before the start rather than reach past it.

  `slurm-discovery` gains the matching `squeue` reason, and a note that
  reservations are weather rather than structure: they recur monthly but each
  one has a date, so they are read live and never written to the cached map.

- Three more skills covering the things an agent hits in the first ten minutes
  of real work on the cluster:

  `bodhi-storage` — `/beevol` is one shared BeeGFS mount and the compute
  node's `/tmp` is a 423G local disk, so inputs are read from the former and
  scratch written to the latter and cleaned up with a `trap`. Slurm hands out
  no private temp directory here (`TMPDIR` is plain `/tmp`, `SLURM_TMPDIR` is
  unset), which is why uncleaned job directories accumulate. It also records
  that `du` on a home directory can run for minutes on BeeGFS, and that at 84%
  full a large write is somebody else's problem too.

  `bodhi-software` — the order is module, then container, then `pixi`/`uv`.
  The tree at `/cluster/software/modules-sw` carries around 137 packages, so
  most of a genomics pipeline is a `module load` away and building it from
  source is wasted time. Pin the version rather than taking `(default)`, load
  inside the job script because `sbatch` starts from a clean login shell, and
  note that `module avail` writes to stderr so grepping it needs `2>&1`.

  `slurm-batch` — for work that is per-sample rather than one command:
  `sbatch` scripts, arrays throttled with `%N`, `--parsable` dependency
  chains, and sizing the next run from `sacct`. Records the local numbers that
  bite: `DefMemPerCPU` is 4000 MB so omitting `--mem` is not "unlimited",
  `MaxArraySize` is 1001 so longer lists need chunking, `kill_invalid_depend`
  is set so a dependent job vanishes rather than hangs when its upstream
  fails, and `MaxRSS` is reported on the step rows where `sacct -X` will not
  show it.

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

[Unreleased]: https://github.com/rnabioco/sinteractive/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/rnabioco/sinteractive/compare/v0.7.0...v1.0.0
[0.7.0]: https://github.com/rnabioco/sinteractive/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/rnabioco/sinteractive/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/rnabioco/sinteractive/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/rnabioco/sinteractive/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/rnabioco/sinteractive/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/rnabioco/sinteractive/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/rnabioco/sinteractive/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/rnabioco/sinteractive/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/rnabioco/sinteractive/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/rnabioco/sinteractive/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/rnabioco/sinteractive/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/rnabioco/sinteractive/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rnabioco/sinteractive/releases/tag/v0.1.0
