# Usage

```bash
sinteractive [LAUNCH OPTIONS] [SBATCH ARGS...]     # launch a session
sinteractive <COMMAND> [ARGS...]
```

A bare `sinteractive` launches; everything else is a subcommand
(`sinteractive --help`, `sinteractive <command> --help`, `man sinteractive`).

## Commands

| Command | What it does |
|---|---|
| `launch` | Launch a new session (the default when no subcommand is given) |
| `attach [TARGET] [--ssh]` | Reattach to a session by JOBID or NAME (your only session when omitted) |
| `ensure NAME` | Reuse the session named NAME, or launch it if absent (implies `--detach`) |
| `status [TARGET]` | Show one session's status |
| `refresh [TARGET]` | Re-check a session's time budget now and update its cache |
| `list` | List running sessions |
| `cancel TARGET` | Cancel a session |
| `queue [--all] [--watch]` | Your job queue: running, pending (with reasons), and recent history |
| `monitor [TARGET\|HOST] [--live]` | Live CPU/GPU/process view of a session's node, or any host |
| `snapshot` | One-shot resource sample of this host |
| `events [TARGET] [--follow] [--since EPOCH]` | Stream session events (NDJSON) |
| `peek TARGET [-n LINES]` | Read the last lines of a session's screen |
| `send TARGET COMMAND` | Type a command into a session's shell |
| `agent-context` | Brief a coding agent on the session it is running inside |
| `quota [--check]` | Storage quota (Bodhi daemons) |
| `hook session-start\|prompt` | Claude Code hook entry points |
| `statusline` | Claude Code statusLine command |
| `mcp` | MCP server over stdio |
| `install-claude` | Install the Claude Code skills, hooks, statusline and MCP server |
| `doctor [--nodes]` | Check this install and, optionally, every compute node |
| `completions SHELL` | Print shell completions (`bash`, `zsh`, `fish`, …) |
| `man` | Print the man page (roff) |
| `schema` | Dump the JSON schemas of the machine-readable outputs |
| `zellij ...` | The embedded zellij's own command line |

`status`, `refresh`, `list`, `cancel`, `queue`, `monitor`, `snapshot`,
`quota`, `doctor` and `launch --detach` take `--json`. `TARGET` is a JOBID or
a session NAME; inside a session it defaults to the current one.

### Launch options

| Option | Description | Default |
|---|---|---|
| `--node NODE` | Request a specific compute node (`--nodelist`) | any available |
| `-p`, `--partition PART` | Slurm partition | `interactive` |
| `-t`, `--time TIME` | Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`) | `24:00:00` |
| `-j`, `--threads N` | Number of CPUs (`--cpus-per-task`) | `2` |
| `-m`, `--mem SIZE` | Memory (`--mem`) | `8G` |
| `-n`, `--name NAME` | Tag the session with a name for easy reattach (`attach NAME`) | |
| `--mouse` | Enable mouse support in the session | on |
| `--no-mouse` | Disable mouse support (overrides `SINTERACTIVE_MOUSE`) | |
| `--detach` | Launch without attaching; print connection info and return | |
| `--json` | Machine-readable JSON output (with `--detach`) | |

All other arguments are passed directly to `sbatch`, in any order, so you can
use any `sbatch` option (`--gres=gpu:1`, `--qos=long`, `--account=...`).
`ensure` takes the same options after the name.

### Examples

```bash
# Default: 1-day session, 2 CPUs, 8G memory
sinteractive

# Named, 8 hours, 4 CPUs, 16G
sinteractive -n rna-seq -t 8h -j 4 -m 16G

# Run on a specific node
sinteractive --node compute01

# GPU session; unknown flags go to sbatch
sinteractive --partition=gpu --gres=gpu:1 --mem=16G

# Longer session on the normal partition (Bodhi: up to 3 days)
sinteractive --time=1-12:00:00 --partition=normal

# Launch without attaching, then come back to it
sinteractive --detach -n build
sinteractive attach build

# What is running, and how long is left?
sinteractive list
sinteractive status build

# The queue, refreshed every 5 s
sinteractive queue --watch

# The build session's node, nvitop-style, from the login node
sinteractive monitor build

# Read the last 40 lines of a session's screen
sinteractive peek build -n 40
```

## Inside a session

`Ctrl+b` is the only chord (tmux muscle memory); press it, then one key.
Everything else zellij binds by default is cleared, so shell and editor keys
pass through untouched. `Ctrl+b h` shows the same legend in the status bar.

| Keys | Action |
|---|---|
| `Ctrl+b d` | Detach — the session keeps running |
| `Ctrl+b h` (or `?`) | Key legend in the bar; again for the next page, `Esc` to close |
| `Ctrl+b n` | Read the notices (quota, trimmed end time, hints) one at a time; `n` for the next, `Esc` back |
| `Ctrl+b m` | Toggle the monitor panel (CPU, memory, GPUs, processes of the job) |
| `Ctrl+b ,` / `Ctrl+b .` | Previous / next host in the monitor panel |
| `Ctrl+b q` | Your queue in a floating pane (`sinteractive queue --watch`); `Ctrl+c` closes it |
| `Ctrl+b c` | New pane |
| `Ctrl+b "` / `Ctrl+b %` | Split down / split right |
| `Ctrl+b x` | Close the focused pane |
| `Ctrl+b z` | Zoom the focused pane |
| `Ctrl+b o`, `Ctrl+b ←↑→↓` | Focus the next pane / a direction |
| `Ctrl+b [` | Scroll mode: `j`/`k` or arrows, `PgUp`/`PgDn`, `d`/`u` half pages, `g`/`G`, `/` search (`n`/`p` next/previous), `e` open the scrollback in `$EDITOR`, `q`/`Esc` to leave |
| `Ctrl+b r` | Resize mode: arrows or `hjkl` grow, `HJKL` shrink, `+`/`-`, `Enter`/`Esc` to leave |
| `Ctrl+b :` | zellij's pane mode (`n`/`d`/`r` new panes, `f` fullscreen, `w` floating, `c` rename pane) |
| `Ctrl+b Ctrl+b` | Send a literal `Ctrl+b` |

### The status bar

```
● sint 31761255 · rusttest · c3cpu-a2-u3-4 · 22m left · jobs 3R · ^b h help
```

- The dot spins while the session is starting, and turns yellow, then red,
  as the walltime runs down (`SINTERACTIVE_WARN_YELLOW` / `_RED`: an hour and
  ten minutes by default). `SINTERACTIVE_GRACE` seconds before the limit the
  session ends itself, so teardown runs cleanly instead of under Slurm's
  SIGKILL.
- `jobs 3R` counts your running (`R`) and pending (`PD`) jobs.
- `▣ N jobs monitorable ^b m` appears when there is a host the monitor panel
  could show and the panel is closed.
- `⚠ N notices ^b n` appears when the session has something to say — a
  quota overage (red, shimmering), a walltime trimmed before a maintenance
  window, a hint to run `install-claude` while Claude Code is running without
  the hooks. Absent when there is nothing to say; `sinteractive status` prints
  the same text from the login node.

Segments drop from the right as the terminal narrows; the job id is the last
to go.

### The monitor panel

`Ctrl+b m` opens a 12-row panel between the shell and the bar with the same
numbers `sinteractive monitor` shows: CPU and memory against the job's
cgroup limits, load, GPUs when there are any, and the busiest processes.
With more than one host to show, `Ctrl+b ,` and `Ctrl+b .` step through them.
`Ctrl+b m` again closes it.

### Mouse, copy and paste

Mouse mode is on by default: scroll with the wheel, click to focus a pane,
drag borders to resize, and select text to copy it — the selection lands in
your local system clipboard over SSH via OSC 52. Hold **Shift** to select
with the terminal instead. `--no-mouse` or `SINTERACTIVE_MOUSE=off` turns
mouse mode off for a session.

For keyboard copying, `Ctrl+b [` enters scroll mode; `e` opens the whole
scrollback in `$EDITOR`, which is the easiest way to search or copy a long
stretch of output.

## Reconnecting after a disconnect

If your SSH connection drops or you detach (`Ctrl+b d`), the session
**keeps running** on the compute node and your work is safe. From the login
node:

```bash
# List your running sessions
sinteractive list
#   JOBID       NAME                  NODE            PARTITION     ELAPSED     TIMELIMIT   CWD
#   12345       rna-seq               compute01       cpu           01:23:45    1-00:00:00  ~/projects/rna-seq

# Reattach
sinteractive attach 12345
sinteractive attach rna-seq
```

If you have only one session running, a bare `sinteractive attach` goes
straight to it — no need to look up the job id first. With several running,
it lists them with ready-to-run commands to pick from.

`attach` reconnects through Slurm (`srun --overlap`), which needs no SSH
access to the node. `attach --ssh` uses `ssh -X` instead, which is the way
to get X11 forwarding.

!!! info "This is the key advantage over `srun --pty bash`"
    With `srun`, a dropped SSH connection kills your session and any running
    processes. With `sinteractive`, you just reconnect and pick up where you
    left off.

!!! note "X11"
    The launch attaches over `ssh -X`, so shells started at launch have a
    `DISPLAY`. A later `attach` goes through `srun` and does not forward X11;
    panes opened after an `attach --ssh` inherit the *server's* environment,
    not the new client's. If you need X11 in a pane, `export DISPLAY=...`
    there yourself, or keep the original connection.

## Cancelling the job

Exiting the last shell (type `exit` or `Ctrl+d` in every pane) ends the
Slurm job. You can also cancel it from the login node, by name or job id:

```bash
sinteractive cancel myproj
sinteractive cancel 12345
scancel 12345              # equivalent, job id only
```

Pressing `Ctrl+c` while a launch is still waiting in the queue cancels the
pending job too.

## Waiting for a job to start

When the cluster is busy your job may sit in the queue. While it does,
`sinteractive` shows why it is waiting (Slurm's pend reason — free resources,
higher-priority jobs ahead of you) and, when Slurm can estimate one, the
expected start time:

```
 ⠹ waiting for free resources — est. start 14:32 (2m elapsed)
```

`sinteractive queue` shows the same for every job you have, plus the last
day's history with a memory right-sizing hint; `--all` adds everyone's jobs
in the partitions you can see.

## What a session is for

A session is a durable place to *work from* — editing, git, scheduler
queries, and keeping long-lived state across SSH drops. It is not a compute
target: the default `interactive` partition is the smallest on the cluster,
and anything heavy you run in the session competes with the shell you are
typing in.

Run work in an allocation sized for it instead:

```bash
# One-off job
srun -p rna -c 8 --mem 32G -t 1:00:00 -J make-test --comment=make-test -- make test

# Sustained work: hold one allocation and reuse it
salloc --no-shell -p rna -c 32 --mem 96G -t 4:00:00 -J cargo-ci --comment=cargo-ci
srun --overlap --jobid=ID -- cargo build --release
scancel ID
```

Name every job in both fields — `-J NAME` and `--comment=NAME`, the same short
descriptive value — so the queue says what is running and why:

```bash
squeue --me -o "%.10i %.20j %.20P %.10M %k"   # %j is the name, %k the comment
```

`SLURM_*` is stripped from a session, so `srun` and `salloc` run from inside
one create their own allocations rather than steps of the session's job.

## Environment variables

Set personal defaults in your `~/.bashrc`; explicit flags always win.

| Variable | Description | Default |
|---|---|---|
| `SINTERACTIVE_TIME` | Default wall time (`8h`, `2d`, `D-HH:MM:SS`) | `24:00:00` |
| `SINTERACTIVE_PARTITION` | Default partition | `interactive` |
| `SINTERACTIVE_QOS` | Default QOS (`--qos`); needed on schedulers that require one | unset |
| `SINTERACTIVE_CPUS` | Default CPU count | `2` |
| `SINTERACTIVE_MEM` | Default memory (`16G`) | `8G` |
| `SINTERACTIVE_MOUSE` | `on`/`1`/`true`/`yes` or `off`/`0`/`false`/`no` | `on` |
| `SINTERACTIVE_CACHE` | State files and the extracted zellij bundle; must be visible from the compute nodes | `$XDG_CACHE_HOME/sinteractive` or `~/.cache/sinteractive` |
| `SINTERACTIVE_THEME` | `dark`, `light`, or `auto` (ask the terminal — but not from inside a session, where the answer would come back too late to use; set it there if your terminal is light) | `auto` |
| `SINTERACTIVE_COLOR` | `auto`/`always`/`never` for CLI output; `NO_COLOR` also honoured | `auto` |
| `SINTERACTIVE_WARN_YELLOW` | Seconds left at which the bar turns yellow | `3600` |
| `SINTERACTIVE_WARN_RED` | Seconds left at which the bar turns red | `600` |
| `SINTERACTIVE_GRACE` | Seconds before the walltime limit at which the session ends itself cleanly | `10` |
| `SINTERACTIVE_POLL` | Seconds between scheduler re-checks in the session (floor 5) | `30` |
| `SINTERACTIVE_AGENT_WARN` | Seconds left below which the Claude Code prompt hook warns | `1800` |
| `SINTERACTIVE_QUOTA_POLL` | Seconds between storage-quota checks (floor 30) | `600` |
| `SINTERACTIVE_QUOTA_FILE` | Pipe-delimited file of hard quotas | `/cluster/scripts/quota_current.txt` |
| `SINTERACTIVE_QUOTA_HOSTS` | Quota daemons to sum usage across | Bodhi's `172.20.8.110-118` |
| `SINTERACTIVE_QUOTA_PORT` | Port those daemons listen on | `9878` |
| `SINTERACTIVE_QUOTA_TIMEOUT` | Seconds to wait for each daemon | `5` |
| `SINTERACTIVE_SHARE` | Where `install-claude` finds the skills (a checkout) | beside the binary |
| `SINTERACTIVE_RUNTIME_DIR` | Node-local directory for the zellij socket and readiness marker | `/tmp` |
| `SINTERACTIVE_JOB_ID`, `SINTERACTIVE_NAME` | Exported *inside* a session; not for you to set | |

```bash
# Example: a bigger default allocation, cache on a filesystem with room
export SINTERACTIVE_MEM=16G
export SINTERACTIVE_CPUS=4
export SINTERACTIVE_CACHE=/projects/$USER/.cache/sinteractive
```

## Configuring for Alpine (CU Boulder)

The defaults match Bodhi, but everything scheduler-specific is overridable.
On [Alpine](https://curc.readthedocs.io/en/latest/clusters/alpine/index.html)
three things differ:

- **CPU partition + QOS** — the general-purpose CPU queue is `acpu` and an
  explicit `--qos` is mandatory. `acpu`/`cpu-normal` are the names that took
  effect with Alpine's **2026-08-05** rename of `amilan`/`normal`; both name
  sets are accepted.
- **`/home` is 2 GB** — put the cache on `/projects`.
- **name clash on `PATH`** — Alpine already provides an older, `screen`-based
  `sinteractive` in `/usr/local/bin`, which is ahead of `~/.local/bin` on
  `PATH`. An `alias` forces your copy to win.

Add this to your `~/.bashrc`:

```bash
# Use the ~/.local/bin copy instead of Alpine's older /usr/local/bin one
alias sinteractive="$HOME/.local/bin/sinteractive"

export SINTERACTIVE_PARTITION=acpu         # CPU queue (was 'amilan' pre-2026-08-05)
export SINTERACTIVE_QOS=cpu-normal         # 1-day max walltime; QOS is required on Alpine
export SINTERACTIVE_CACHE=/projects/$USER/.cache/sinteractive
```

Then `sinteractive` launches a 1-day CPU session. For a longer run (up to
7 days), override the QOS: `sinteractive --time=2d --qos=cpu-long`. The default
account (`amc-general` for most users) is applied automatically; pass
`--account=<name>` if you need a different allocation. Alpine has no quota
daemons, so `sinteractive quota` reports "unavailable" there — `curc-quota`
is the tool.

## Configuring for Bodhi

Nothing to set: the built-in defaults are Bodhi's (`interactive` partition,
no QOS, the quota daemons and `/cluster/scripts/quota_current.txt`). Longer
sessions go to the `normal` partition (`sinteractive -t 1-12:00:00 -p
normal`), GPU work to `gpu` (`-p gpu --gres=gpu:1`).

Over-quota sessions carry a red `QUOTA over by …` notice, checked every ten
minutes against the cluster's quota daemons. The check is cached per user,
not per session, so six open sessions do not mean six times the polling.
After deleting something, don't wait out the interval:

```bash
sinteractive quota --check      # re-checks now, updates every open session
# OVER QUOTA: 30.2T of 30T used (100.7%), over by 204.8G
# Quota OK: 24.1T of 30T used (80.3%)
```

## Tab completion

`make install` installs completions for bash, zsh and fish (generated by the
binary itself: `sinteractive completions bash|zsh|fish`). Start a new shell
after installing to pick them up. zsh needs `~/.local/share/zsh/site-functions`
on `$fpath`.
