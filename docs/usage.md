# Usage

```bash
sinteractive [OPTIONS] [SBATCH_ARGS...]
```

## Options

| Option | Description | Default |
|---|---|---|
| `--node NODE` | Request a specific compute node | any available |
| `--partition PART` | SLURM partition | `interactive` |
| `--time TIME` | Wall time limit (supports `8h`, `30m`, `1d12h`, …) | `1 day` |
| `-j`, `--threads N` | Number of CPUs (alias for `--cpus-per-task`) | `2` |
| `-m`, `--mem SIZE` | Memory | `8G` |
| `-n`, `--name NAME` | Tag the session with a name for easy reattach (`--attach NAME`) | |
| `--mouse` | Enable tmux mouse support (scroll, click panes, drag to resize) | off |
| `--no-mouse` | Disable mouse support (overrides `SINTERACTIVE_MOUSE`) | |
| `--detach` | Launch without attaching; print connection info and return | |
| `--status [TARGET]` | Show session status by JOBID or NAME (state, node, time remaining) | current session |
| `--json` | With `--list`/`--status`/`--detach`: machine-readable JSON output | |
| `-a`, `--attach [TARGET]` | Reattach by JOBID or NAME; with no target, your only session | |
| `--ensure NAME` | Reuse the session named NAME, or launch it if absent (implies `--detach`) | |
| `--cancel TARGET` | Cancel a session by JOBID or NAME | |
| `--agent-context` | Brief a coding agent on the session it is running inside | |
| `-l`, `--list` | List running sinteractive sessions | |
| `-h`, `--help` | Show help message | |
| `-V`, `--version` | Show version | |

All other arguments are passed directly to `sbatch`, so you can use any
`sbatch` option. See `man sinteractive` for full documentation.

## Environment variables

Set personal defaults in your `~/.bashrc`; explicit flags always win.

| Variable | Description | Default |
|---|---|---|
| `SINTERACTIVE_TIME` | Default wall time (e.g. `8h`, `2d`) | `1 day` |
| `SINTERACTIVE_PARTITION` | Default partition | `interactive` |
| `SINTERACTIVE_QOS` | Default QOS (`--qos`); needed on schedulers that require one | unset |
| `SINTERACTIVE_CPUS` | Default CPU count | `2` |
| `SINTERACTIVE_MEM` | Default memory (e.g. `16G`) | `8G` |
| `SINTERACTIVE_MOUSE` | `on`/`1`/`true`/`yes` enables mouse support | off |
| `SINTERACTIVE_TMUX` | Path to the `tmux` binary on the compute node | `/usr/local/bin/tmux` |

```bash
# Example: always use mouse mode and a bigger default allocation
export SINTERACTIVE_MOUSE=on
export SINTERACTIVE_MEM=16G
export SINTERACTIVE_CPUS=4
```

## Configuring for other clusters (example: CU Alpine)

The defaults above match Bodhi, but everything scheduler-specific is
overridable. To run on
[CU Boulder's Alpine](https://curc.readthedocs.io/en/latest/clusters/alpine/index.html),
three things differ:

- **tmux path** — Alpine ships tmux as a system package at `/usr/bin/tmux`,
  not the source-built `/usr/local/bin/tmux` Bodhi uses.
- **CPU partition + QOS** — the general-purpose CPU queue is `acpu` (an
  explicit `--qos` is mandatory). `acpu`/`cpu-normal` are the names that take
  effect after Alpine's **2026-08-05** rename of `amilan`/`normal`; both name
  sets are already accepted, so using the new ones now means no change at the
  cutover.
- **name clash on `PATH`** — Alpine already provides an older, `screen`-based
  `sinteractive` in `/usr/local/bin`, which is ahead of `~/.local/bin` on
  `PATH`. An `alias` forces your copy to win.

After `make install`, add this to your `~/.bashrc`:

```bash
# Use the ~/.local/bin copy instead of Alpine's older /usr/local/bin one
alias sinteractive="$HOME/.local/bin/sinteractive"

export SINTERACTIVE_TMUX=/usr/bin/tmux     # Alpine's system tmux
export SINTERACTIVE_PARTITION=acpu         # CPU queue (was 'amilan' pre-2026-08-05)
export SINTERACTIVE_QOS=cpu-normal         # 1-day max walltime; QOS is required on Alpine
```

Then `sinteractive` launches a 1-day CPU session. For a longer run (up to
7 days), override the QOS: `sinteractive --time=2d --qos=cpu-long`. The default
account (`amc-general` for most users) is applied automatically; pass
`--account=<name>` if you need a different allocation.

## What a session is for

A session is a durable place to *work from* — editing, git, scheduler
queries, and keeping long-lived state across SSH drops. It is not a compute
target: the default `interactive` partition is the smallest on the cluster,
and anything heavy you run in the session competes with the shell you are
typing in.

Run work in an allocation sized for it instead:

```bash
# One-off job
srun -p rna -c 8 --mem 32G -t 1:00:00 -- make test

# Sustained work: hold one allocation and reuse it
salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00   # job id on stderr
srun --overlap --jobid=ID -- cargo build --release
scancel ID
```

`SLURM_*` is stripped from a session, so `srun` and `salloc` run from inside
one create their own allocations rather than steps of the session's job.

## Examples

```bash
# Default: 1-day session, 2 CPUs, 8G memory
sinteractive

# Run on a specific node
sinteractive --node compute01

# 2-hour session on the rna partition
sinteractive --time=2:00:00 --partition=rna

# Override default memory and CPUs
sinteractive --mem=16G --cpus-per-task=4

# GPU session
sinteractive --partition=gpu --gpus=1 --mem=16G

# Longer session on the normal partition (up to 3 days)
sinteractive --time=1-12:00:00 --partition=normal
```

## Reconnecting after a disconnect

If your SSH connection drops or you intentionally detach (`Ctrl-b d`), the
tmux session **keeps running** on the compute node and your work is safe. To
reconnect from the login node:

```bash
# List your running sessions
sinteractive --list
#   JOBID       NAME                  NODE            PARTITION     ELAPSED     TIMELIMIT   CWD
#   12345       rna-seq               compute01       cpu           01:23:45    1-00:00:00  ~/projects/rna-seq

# Reattach
sinteractive --attach 12345
```

If you have only one session running, a bare `sinteractive --attach` goes
straight to it — no need to look up the job id first. With several running,
it lists them with ready-to-run commands to pick from.

Sessions launched with `-n NAME` can be reattached by name
(`sinteractive --attach NAME`). Forgot to name one? Press `Ctrl-b $` inside
the session to name (or rename) it in place — the new name shows up in the
status bar, `squeue`, `--list`, and works with `--attach NAME`.

!!! info "This is the key advantage over `srun --pty bash`"
    With `srun`, a dropped SSH connection kills your session and any running
    processes. With `sinteractive`, you just reconnect and pick up where you
    left off.

!!! note "X11 after reattaching"
    X11 forwarding is set up on the **initial** connection (`ssh -X`).
    Reattaching with `--attach` reconnects through Slurm (`srun`) rather than
    a new `ssh -X`, so GUI apps launched **after** a reattach won't have a
    working `DISPLAY`. If you need X11, keep the original connection, or start
    a fresh session for GUI work.

## Tips

### Basic tmux commands

| Action | Key |
|---|---|
| Show help popup (job info, keys) | `Ctrl-b h` |
| Detach from session | `Ctrl-b d` |
| Name/rename session (updates squeue and `--attach` name) | `Ctrl-b $` |
| Split pane horizontally | `Ctrl-b "` |
| Split pane vertically | `Ctrl-b %` |
| Switch between panes | `Ctrl-b arrow-key` |

### Scrollback and keyboard copy/paste

Copy mode uses vi keys (forced, regardless of `$EDITOR`), and copied text
lands in your **local** system clipboard over SSH via OSC 52 — usually far
smoother than mouse selection, especially with the scrollbar active.

| Action | Key |
|---|---|
| Enter copy mode (scrollback) | `Ctrl-b [` (press `q` to exit) |
| Move around | arrow keys, `PgUp`/`PgDn`, `g`/`G` for top/bottom |
| Start selection | `Space` (`V` selects whole lines) |
| Copy and exit copy mode | `Enter` (also lands in your local clipboard) |
| Paste into a pane | `Ctrl-b ]` |
| Search up / down | `?` / `/` (then `n`/`N` for next/previous match) |

!!! tip "Mouse support"
    Start with `sinteractive --mouse` to scroll with the wheel, click to switch
    panes, and drag borders to resize. Mouse mode captures terminal selection,
    so hold **Shift** when you want to select text for an OS-level copy (tmux's
    own mouse selection is copied out over SSH automatically).

### Cancelling the job

Exiting the tmux session (type `exit` or `Ctrl-d` in all panes) automatically
cancels the SLURM job. You can also cancel it from the login node, by name or
job id:

```bash
sinteractive --cancel myproj
sinteractive --cancel 12345
scancel 12345              # equivalent, job id only
```

Pressing `Ctrl-C` while a launch is still waiting in the queue cancels the
pending job too.

### Waiting for a job to start

When the cluster is busy your job may sit in the queue. While it does,
`sinteractive` shows why it is waiting (Slurm's pend reason — free resources,
higher-priority jobs ahead of you) and, when Slurm can estimate one, the
expected start time:

```
 ⠹ waiting for free resources — est. start 14:32 (2m elapsed)
```

### Tab completion

`make install` installs a bash completion. It completes options, and after
`--attach`, `--status`, and `--cancel` it completes the job ids and names of
your running sessions:

```bash
sinteractive --attach <TAB>
# 12345  rna-seq  12346  assembly
```

Session names are read from the state files in `~/.cache/sinteractive/`, not
from `squeue`, so completion stays instant even when the scheduler is slow.
Start a new shell after installing to pick it up.
