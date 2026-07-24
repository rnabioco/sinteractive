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

This installs the script and man page to `/usr/local` on every compute node.
If the checkout lives on a cluster-wide mount and `pdsh` is available, each
node installs straight from the shared path; otherwise it falls back to an
scp/ssh loop. `NODES` and `SSH_USER` are overridable as above.
