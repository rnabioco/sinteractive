# sinteractive

Persistent interactive sessions on Slurm compute nodes, with
[zellij](https://zellij.dev) compiled in.

**Docs:** <https://rnabioco.github.io/sinteractive/>

`sinteractive` submits a batch job that starts a zellij server on the
allocated node, then connects you to it. Because the shell lives in a
multiplexer, the session survives SSH drops and can be reattached later. It
is a single static-ish binary: zellij, the status bar, the monitor panel and
the Slurm plumbing are all inside it, so there is nothing to install on the
compute nodes and no multiplexer to find there.

It is also built for coding agents as much as for people. Every reporting
command has a `--json` form, `session peek`/`send` read and drive a session
from the login node, `session events` streams what happens in one, and
`claude install` wires
[Claude Code](https://code.claude.com/docs/) up with skills, hooks, a
statusline and an MCP server.

It is a clean-room reimplementation inspired by the original `sinteractive`
by Pär Andersson (NSC, Sweden) and the CU Boulder adaptation by Jonathon
Anderson, developed for the [Bodhi cluster](https://rnabioco.github.io/bodhi-docs/)
at the RNA Bioscience Initiative and for CU Boulder's Alpine — but it is
cluster-agnostic: scheduler details are driven by `SINTERACTIVE_*`
environment variables.

## Why use `sinteractive` instead of `srun --pty bash`?

| | `srun --pty bash` | `sinteractive` |
|---|---|---|
| Survives SSH disconnects | No — session is lost | Yes — the zellij server keeps it alive |
| Reconnect to session | Not possible | `sinteractive attach JOBID\|NAME` |
| Multiple panes | No | Yes — splits, zoom, scrollback (`Ctrl+b`) |
| Mouse and copy | Terminal's own | Mouse on by default; select-to-copy lands in your local clipboard |
| Status bar | None | Job id, node, walltime left, your queue, notices (`⚠ N notices`) |
| Monitor panel | None | `Ctrl+b m`: CPU, memory and GPU bars for every job you can see, in-session; `t` for the full process view |
| Remote read/drive | None | `sinteractive session peek` / `send` from the login node or an agent |
| X11 forwarding | Manual setup | `attach --ssh` (`ssh -X`) |

> [!TIP]
> Use `srun --pty bash` for quick, throwaway interactive work. Use
> `sinteractive` when you need a session that persists through network
> interruptions, or a place an agent can observe and reach.

## Installation

Download the binary from the
[releases page](https://github.com/rnabioco/sinteractive/releases) (built on
Rocky 8, glibc 2.28, x86_64) and put it on your `PATH`:

```bash
mkdir -p ~/.local/bin
mv sinteractive-x86_64-linux-gnu-glibc2.28 ~/.local/bin/sinteractive
chmod +x ~/.local/bin/sinteractive
sinteractive doctor          # is this install able to run a session from here?
```

Or build from a checkout:

```bash
git clone https://github.com/rnabioco/sinteractive
cd sinteractive
make build      # cargo build --release -p sint --features web_server_capability
make install    # binary, man page, completions and the Claude Code assets
```

As a regular user `make install` copies the binary, man page and shell
completions to `~/.local/bin`, `~/.local/share/man` and
`~/.local/share/{bash-completion,zsh/site-functions}` (make sure
`~/.local/bin` is on your `$PATH`); as root it installs to `/usr/local`
instead. `make install PREFIX=~/bin` picks another location. The binary
itself lands as `.sinteractive-<sha>` beside a `sinteractive` symlink, and
earlier builds stay until no session can be running them — reinstalling
while sessions are up is safe, even on an NFS home where replacing the
executable in place would kill them with SIGBUS.

Building needs a Rust toolchain (`rust-toolchain.toml` pins stable and adds
the `wasm32-wasip1` target, which the status plugin is built for), a C/C++
toolchain with `cmake` and `perl`, and the libcurl and OpenSSL headers
(`libcurl-devel openssl-devel` on Rocky). The binary links glibc, so build it
on the oldest glibc it must run on — the release builds run in a
`rockylinux:8` container for that reason — and it needs `libcurl.so.4` at
runtime.

Requirements at runtime: a Slurm cluster, and the binary on a filesystem the
compute nodes can see (the batch job execs it from wherever it is installed).
There is nothing to fan out to the nodes: the zellij server is the binary
itself, and its plugin and config are extracted once into the cache directory,
which is shared too. `attach` goes through `srun --overlap`, so SSH access to
the nodes is only needed for `attach --ssh`, `peek`, `send`, `monitor --live`
and `doctor --nodes`.

> [!NOTE]
> On Alpine, `/home` is 2 GB and `~/.cache` is where the state files and the
> extracted bundle go by default. Point the cache somewhere with room:
> `export SINTERACTIVE_CACHE=/projects/$USER/.cache/sinteractive`.

## Usage

```bash
sinteractive [LAUNCH OPTIONS] [SBATCH ARGS...]     # launch a session
sinteractive <COMMAND> [ARGS...]
```

A bare `sinteractive` launches; everything else is a subcommand
(`sinteractive --help`, `sinteractive <command> --help`, `man sinteractive`).

### Sessions

| Command | What it does |
|---|---|
| `launch` | Launch a new session (the default when no subcommand is given) |
| `attach [TARGET] [--ssh]` | Reattach to a session by JOBID or NAME (your only session when omitted) |
| `list` | List running sessions |
| `status [TARGET] [--refresh]` | Show one session's status; `--refresh` re-checks its time budget against Slurm first |
| `cancel TARGET` | Cancel a session |

### Watching

| Command | What it does |
|---|---|
| `queue [--all] [--watch]` | Your job queue: running, pending (with reasons), and recent history |
| `monitor [TARGET\|HOST] [--live] [--once]` | Live CPU/GPU/process view of a session's node, or any host; `--once` prints one sample of this host and exits |
| `quota [--check]` | Storage quota (Bodhi daemons) |
| `doctor [--nodes]` | Check this install and, optionally, every compute node |

### Driving a session from outside

A person at a prompt attaches; a script or an agent reaches in with these.

| Command | What it does |
|---|---|
| `session ensure NAME` | Reuse the session named NAME, or launch it if absent (implies `--detach`) |
| `session peek TARGET [-n LINES]` | Read the last lines of a session's screen |
| `session send TARGET COMMAND` | Type a command into a session's shell |
| `session events [TARGET] [--follow] [--since EPOCH]` | Stream session events (NDJSON) |

### Claude Code

| Command | What it does |
|---|---|
| `claude install` | Install the Claude Code skills, hooks, statusline and MCP server |
| `claude context` | Brief a coding agent on the session it is running inside |
| `claude hook session-start\|prompt` | Hook entry points (Claude Code runs these) |
| `claude statusline` | statusLine command (Claude Code runs this) |
| `claude mcp` | MCP server over stdio (Claude Code runs this) |

`claude install` writes the last four into your Claude Code settings; you
never type them yourself.

### Generated output

| Command | What it does |
|---|---|
| `gen completions SHELL` | Shell completions |
| `gen man` | The man page (roff) |
| `gen schema` | The JSON schemas of the machine-readable outputs |
| `zellij ...` | The embedded zellij's own command line |

`status`, `list`, `cancel`, `queue`, `monitor`, `quota`, `doctor` and
`launch --detach` take `--json`. `TARGET` is a JOBID or a session NAME;
inside a session it defaults to the current one.

The pre-grouping spellings — `ensure`, `peek`, `send`, `events`, `refresh`,
`snapshot`, `agent-context`, `hook`, `statusline`, `mcp`, `install-claude`,
`completions`, `man` and `schema` as top-level commands — still work but are
hidden from `--help`.

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

### Examples

```bash
# Default: 1-day session, 2 CPUs, 8G memory
sinteractive

# Named, 8 hours, 4 CPUs, 16G
sinteractive -n rna-seq -t 8h -j 4 -m 16G

# GPU session; unknown flags go to sbatch
sinteractive --partition=gpu --gres=gpu:1 --mem=16G

# Launch without attaching, then come back to it
sinteractive --detach -n build
sinteractive attach build

# What is running, and how long is left?
sinteractive list
sinteractive status build

# Read the last 40 lines of a session's screen from the login node
sinteractive session peek build -n 40
```

### Inside a session

`Ctrl+b` is the only chord (tmux muscle memory); press it, then one key.
`Ctrl+b h` shows the same legend in the status bar.

| Keys | Action |
|---|---|
| `Ctrl+b d` | Detach — the session keeps running |
| `Ctrl+b h` (or `?`) | Key legend in the bar; again for the next page, `Esc` to close |
| `Ctrl+b n` | Read the notices (quota, trimmed end time, hints) one at a time; `Ctrl+b n` again for the next, `Ctrl+b Esc` closes |
| `Ctrl+b m` | Focus the monitor panel (CPU, memory and GPU bars), opening it if it is closed; again to hand the focus back to the shell. In the panel: `←`/`→` job, `t` the full monitor TUI, `esc` back to the shell, `x` close |
| `Ctrl+b ,` / `Ctrl+b .` | Previous / next job in the monitor panel, without focusing it |
| `Ctrl+b q` | Your queue in a floating pane (`sinteractive queue --watch`) — running, pending and the last 24 h; `q` or `Esc` closes it, `r` refreshes (the pane says so on its second line) |
| `Ctrl+b c` | New pane |
| `Ctrl+b "` / `Ctrl+b %` | Split down / split right |
| `Ctrl+b x` | Close the focused pane |
| `Ctrl+b z` | Zoom the focused pane |
| `Ctrl+b o`, `Ctrl+b ←↑→↓` | Focus the next pane / a direction |
| `Ctrl+b [` | Scroll mode: `j`/`k`, `PgUp`/`PgDn`, `g`/`G`, `/` search, `e` open scrollback in `$EDITOR`, `q`/`Esc` to leave |
| `Ctrl+b r` | Resize mode: arrows or `hjkl`, `Enter`/`Esc` to leave |
| `Ctrl+b :` | zellij's pane mode |
| `Ctrl+b Ctrl+b` | Send a literal `Ctrl+b` |

The status bar reads
`● sint 31761255 · rusttest · c3cpu-a2-u3-4 · 22m left · jobs 3R · ^b h help`:
the dot spins while the session is starting and turns yellow, then red, as
the walltime runs down (`SINTERACTIVE_WARN_YELLOW`/`_RED`); `jobs` counts
your running and pending jobs; a `⚠ N notices` counter appears when the
session has something to say (red while a quota overage is among them).
Segments drop from the right as the terminal narrows.

Mouse mode is on by default: scroll with the wheel, click to focus a pane,
drag borders to resize, and select text to copy it (it lands in your local
clipboard over SSH). Hold **Shift** to select with the terminal instead.
`--no-mouse` or `SINTERACTIVE_MOUSE=off` turns it off.

Exiting the last shell (`exit`, `Ctrl+d`) ends the job. From the login node,
`sinteractive cancel NAME|JOBID` (or `scancel`) does the same; `Ctrl+c` while
a launch is still waiting in the queue cancels the pending job.

### Environment variables

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
| `SINTERACTIVE_MONITOR_SESSIONS` | Show your *other* sinteractive sessions in the monitor panel alongside your real jobs | `off` |
| `SINTERACTIVE_AGENT_WARN` | Seconds left below which the Claude Code prompt hook warns | `1800` |
| `SINTERACTIVE_QUOTA_POLL` | Seconds between storage-quota checks (floor 30) | `600` |
| `SINTERACTIVE_QUOTA_FILE` | Pipe-delimited file of hard quotas | `/cluster/scripts/quota_current.txt` |
| `SINTERACTIVE_QUOTA_HOSTS` | Quota daemons to sum usage across | Bodhi's `172.20.8.110-118` |
| `SINTERACTIVE_QUOTA_PORT` | Port those daemons listen on | `9878` |
| `SINTERACTIVE_QUOTA_TIMEOUT` | Seconds to wait for each daemon | `5` |
| `SINTERACTIVE_SHARE` | Where `claude install` finds the skills (a checkout) | beside the binary |
| `SINTERACTIVE_RUNTIME_DIR` | Node-local directory for the zellij socket and readiness marker | `/tmp` |
| `SINTERACTIVE_JOB_ID`, `SINTERACTIVE_NAME` | Exported *inside* a session; not for you to set | |

```bash
# Example: a bigger default allocation, cache on a filesystem with room
export SINTERACTIVE_MEM=16G
export SINTERACTIVE_CPUS=4
export SINTERACTIVE_CACHE=/projects/$USER/.cache/sinteractive
```

### Configuring for Alpine (CU Boulder)

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

### Configuring for Bodhi

Nothing to set: the built-in defaults are Bodhi's (`interactive` partition,
no QOS, the quota daemons and `/cluster/scripts/quota_current.txt`). Longer
sessions go to the `normal` partition (`sinteractive -t 1-12:00:00 -p
normal`), GPU work to `gpu` (`-p gpu --gres=gpu:1`). Over-quota sessions carry
a red `QUOTA over by …` notice, re-checked every ten minutes; after freeing
space, `sinteractive quota --check` re-checks now and updates every open
session.

## How it works

1. **Submits a batch job** — `sbatch --wrap "exec sinteractive __job …"`,
   tagged with `Comment=sinteractive[:NAME]` so sessions are found by that
   marker rather than by name. If a maintenance reservation would block the
   request, the walltime is trimmed to end before it (and the session says so).
2. **Waits for the job to start** — polls `squeue`, showing Slurm's pend
   reason and estimated start time. `Ctrl+c` here cancels the pending job.
3. **Starts zellij on the node** — the job body brings up a headless zellij
   server (the embedded one), with every `SLURM_*` variable stripped from the
   session's environment, then runs a sampler that keeps the status bar, the
   state file, the notices, the metrics snapshot and the event log current.
4. **Attaches** — through `srun --overlap --pty` by default, or `ssh -X` with
   `attach --ssh`. Detaching or losing the connection leaves the job running;
   exiting the last shell ends it.

Everything the login-node commands need — `status`, `list`, `monitor`,
`statusline`, the MCP server — is read from the shared cache directory the
session writes to, so they cost no SSH and mostly no scheduler queries. See
[Deploying on a cluster](https://rnabioco.github.io/sinteractive/deploy/) for
the file layout.

## Scripting and agents

Every reporting command has a `--json` form, `ensure NAME` is an idempotent
get-or-create, `peek`/`send` read and drive a session from outside, `events
--follow` streams what happens in one, and the state file
`<cache>/JOBID.json` carries `remaining_seconds` for cheap polling. See
[Scripting & Agents](https://rnabioco.github.io/sinteractive/scripting/)
(`docs/scripting.md`) for the contracts and the rule that matters most: **a
session is not a compute target**.

## Claude Code integration

```bash
sinteractive claude install   # from any installed copy
make claude-install           # equivalent, from a checkout
```

This installs the six skills (`hpc-compute`, `slurm-discovery`, `hpc-storage`,
`hpc-software`, `slurm-batch`, `git-workflow`) into `~/.claude/skills/`, then
registers in your `settings.json` the two hooks (`sinteractive claude hook
session-start` briefs the agent on the session it is in; `sinteractive claude hook
prompt` warns when walltime is short), the statusline (`sinteractive
statusline`, which shows the model, context usage and the working directory
under the input box; session state stays on the status bar) and the MCP server (`sinteractive claude mcp`, via `claude mcp
add`), each by the absolute path of the binary that ran the install, so PATH order in Claude Code's
non-interactive shell cannot hand them to some other `sinteractive`. The merge is additive, idempotent and backed up; a `settings.json` that
does not parse is left alone and the snippet printed instead. Old 0.x hook
scripts are removed and their entries replaced.

`sinteractive claude context` prints the briefing by hand, so you can see
exactly what the agent is told.

## Migrating from 0.x

- **Subcommands.** `--status`, `--list`, `--attach`, `--ensure`, `--cancel`,
  `--refresh`, `--check-quota`, `--agent-context` and `--install-claude` are
  now `status`, `list`, `attach`, `session ensure`, `cancel`, `status --refresh`,
  `quota --check`, `claude context` and `claude install`. The old flags are
  accepted for one release and warn on stderr.
- **Grouped.** Everything that wires sinteractive into Claude Code lives under
  `claude` (`install`, `context`, `hook`, `statusline`, `mcp`), and the
  generated output under `gen` (`completions`, `man`, `schema`). `refresh`
  became `status --refresh` and `snapshot` became `monitor --once`. Every old
  spelling still resolves; none of them show in `--help`.
- **No tmux.** zellij is compiled in; `SINTERACTIVE_TMUX` is gone and nothing
  needs installing on the compute nodes. The `make tmux*` and `nodes-check`
  targets are gone with it. Keys are the same `Ctrl+b` chords, except that
  in-session rename (`Ctrl+b $`) is not available yet — name sessions at
  launch with `-n`.
- **Mouse is on by default.** `--no-mouse` or `SINTERACTIVE_MOUSE=off` to
  turn it off.
- **Hooks are native.** `claude install` replaces the
  `sinteractive-*.sh` hook scripts with `sinteractive claude hook …` and also
  registers the statusline and the MCP server.
- **Attach goes through `srun --overlap`** rather than ssh; `attach --ssh` is
  the old path (and the one that forwards X11).
- **The state-file contract is unchanged** (`<cache>/JOBID.json`, same fields,
  same order), so anything that polls it keeps working. The cache directory
  gained `bin/` (the extracted zellij bundle), `xdg/` (zellij's own cache),
  `JOBID.metrics.json` and `JOBID.events.ndjson`.

## Docs development

The [documentation site](https://rnabioco.github.io/sinteractive/) is built
with [zensical](https://zensical.org) from `docs/` and deploys to GitHub Pages
on every push to `main`. Requires [pixi](https://pixi.sh):

```bash
pixi run docs    # serve locally at http://localhost:8000
pixi run build   # build the site (strict mode)
```

## License

MIT — see [LICENSE](LICENSE). The embedded zellij is MIT-licensed too; see
`crates/sint/src/zellij_embed/LICENSE-zellij.md`.
