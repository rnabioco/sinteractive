# Deploying on a cluster

`sinteractive` runs tmux **on the allocated compute node**, and `/usr/local`
is typically node-local, so the tmux binary (and, for a system-wide install,
the script itself) must exist on every node. The Makefile has root-only admin
targets for this.

## Building and distributing tmux

sinteractive needs tmux ≥ 3.7 on every compute node (path configurable via
`SINTERACTIVE_TMUX`). These targets build the latest
[tmux release](https://github.com/tmux/tmux/wiki) from source on the head node
and fan it out to the cluster:

```bash
# One-time: install build dependencies (RHEL/Rocky 9)
sudo make tmux-deps

# Download, build, and install into /usr/local
sudo make tmux

# Copy the built binary to every Slurm compute node
sudo make tmux-push

# Or do the build + push in one step
sudo make tmux-all
```

- Bump the version with `TMUX_VERSION`, e.g. `sudo make tmux TMUX_VERSION=3.8`.
- Restrict the push to specific nodes with `NODES`, e.g.
  `sudo make tmux-push NODES="compute00 compute01"` (defaults to all Slurm
  nodes from `sinfo`).
- `tmux-push` copies to a temp name and renames into place, so running
  `sinteractive` sessions aren't disturbed.

## Installing sinteractive on every node

```bash
sudo make nodes
```

This installs the same set of files as `make install-system` — script, man
page, bash completion, and the Claude Code assets under
`/usr/local/share/sinteractive` — to `/usr/local` on every compute node. (The
head node itself is covered by `sudo make install-system`.)

The assets belong on the compute nodes and not only on the head node:
`sinteractive --install-claude` finds them relative to the running script, so
someone who runs it from inside a session is running the node's copy, and
without `<prefix>/share/sinteractive` beside it that call fails.

If the checkout lives on a cluster-wide mount and `pdsh` is available, each
node installs straight from the shared path; otherwise it falls back to
piping a tar to each node in turn. `NODES` and `SSH_USER` are overridable as
above.

The `pdsh` call asks for the ssh rcmd module by name (`-R ssh`). pdsh's own
default is `rsh`, which on a cluster with nothing listening on port 514
answers `connect: Connection refused` for every node at once — and a
`PDSH_RCMD_TYPE=ssh` exported in root's shell does not rescue `sudo make
nodes`, because sudo resets the environment. Override with
`make nodes PDSH_RCMD=<module>`; `pdsh -V` lists what is compiled in. The script is renamed into place rather than written over, for the
same reason `tmux-push` does it: `--attach` SSHes into a node and runs the
script there, so a copy may be executing while you install.

## Checking what is deployed

```bash
make nodes-check
# compute00    sinteractive=0.3.0  assets=yes  tmux 3.7c
# compute01    sinteractive=unknown  assets=no  tmux 3.7b
# compgpu01    unreachable
```

Read-only and unprivileged, so anyone can run it. Worth doing after a
fan-out, because drift is otherwise invisible: `sbatch` spools the submitted
copy of the script, so sessions keep working from whatever the *submitting*
node has, and a stale compute node only shows up in `--attach` (which runs
the script on the node) and in `--install-claude`. `sinteractive=unknown`
means a copy old enough to predate the `VERSION` string.
