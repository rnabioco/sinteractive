# Compute on Alpine (CU Boulder / CURC)

Surveyed live **2026-08-27** — re-verify with the commands at the end when
something is rejected that this file says is fine.

## QOS is mandatory

Every `srun`/`salloc`/`sbatch` needs `--qos` alongside `-p`. The general
partitions have `AllowAccounts=ALL`, and the default account (`amc-general`
for this user) works everywhere general — so a rejection is almost always
about the QOS/partition pair, not the account. Partitions and QOS were
renamed on 2026-08-05 (`amilan`→`acpu`, `normal`→`cpu-normal`, …); both name
sets are currently accepted, and older docs and scripts may use either.

## Partitions and their QOS

| Partition | QOS | MaxWall | Shape |
|---|---|---|---|
| `acpu` (default) | `cpu-normal` / `cpu-long` | 24h / 7d | 420 nodes, 64c / ~240G each. DefaultTime 4h. |
| `amem` | `mem-normal` / `mem-long` | 24h / 7d | High-memory nodes. |
| `acompile` | `compile` | 12h | Builds. 1 node, ≤8 CPUs and 4 jobs per user. |
| `atesting` | `testing` | 1h | Quick tests, ≤16 CPUs, 1 job. |
| `aa100` `ah200` `al40` `ami100` `artxpro6000` `gh200` | `gpu-normal` / `gpu-long` / `gpu-testing` | 24h / 7d / 1h | GPU nodes, one partition per GPU type. |
| `dtn` | `dtn` | — | Data transfer nodes. |

```bash
srun -p acpu --qos=cpu-normal -c 8 --mem 30G -t 4:00:00 \
  -J bwa-align --comment=bwa-align -- bwa ...
```

The `*-long` QOS are how you get past the 24h ceiling (7 days; `cpu-long`
caps you at 20 nodes and carries a priority boost).

## Memory is coupled to CPUs on `acpu`

`DefMemPerCPU=MaxMemPerCPU=3840M`. You cannot buy more memory per CPU; a
`--mem` above `-c` × 3.75G makes Slurm raise the CPU count to cover it (and
bill for it). For memory-bound work, size CPUs as `mem / 3.75G` and expect
no benefit from asking otherwise.

## sinteractive on Alpine

sinteractive works as described in SKILL.md; nothing needs installing on
the nodes (zellij is inside the binary). There is no dedicated interactive
partition; sessions land on `acpu` via `SINTERACTIVE_PARTITION`/
`SINTERACTIVE_QOS` (this user sets `acpu`/`cpu-normal` in their bashrc,
along with `SBATCH_PARTITION`/`SBATCH_QOS` so plain `sbatch` inherits the
same defaults). An `interactive` QOS exists (12h, ≤16 CPUs, 1 job) but is
not what the user's setup uses. `/home` is 2 GB, so the session state and
the extracted zellij bundle belong on `/projects`:
`SINTERACTIVE_CACHE=/projects/$USER/.cache/sinteractive`.

## Worktrees and build trees live on scratch

`/projects` is the small, backed-up tier, and a git worktree is a throwaway
build tree, so worktrees belong on `/scratch/alpine/$USER`. With
sinteractive's Claude Code hooks registered (`sinteractive claude install`)
that is automatic for every repository: `EnterWorktree` creates
`/scratch/alpine/$USER/worktrees/<repo>/<name>` (`SINTERACTIVE_WORKTREES`
moves the root), and `git worktree list` shows where a worktree is. Do not
symlink a repository's `.claude/worktrees` onto scratch instead — Claude
Code refuses to create a worktree through a symlink. Build output follows
the same rule: `CARGO_TARGET_DIR=/scratch/alpine/$USER/.cache/cargo-target`
(this user's shell sets it), and `make install` reads it, so the binary is
found where cargo put it. A worktree that must move filesystems cannot be
`git worktree move`d across them: `cp -a` it, remove the old directory, then
`git worktree repair NEWPATH`.

There is no recurring all-node maintenance pattern to assume, but
`scontrol show reservation` before long walltime still applies — the
deferred-past-the-window trap in SKILL.md works identically wherever a
reservation exists.

## Re-verify when the map drifts

```bash
sinfo -o "%20P %5a %12l %6D %6t"                          # partitions, ceilings
scontrol show partition acpu                              # QOS pairs, mem-per-CPU
sacctmgr -nP show qos format=Name,MaxWall,MaxTRESPU,MaxSubmitJobsPU,Flags |
  grep -E '^(cpu|mem|gpu)-|^(compile|testing|interactive)\|'
sacctmgr -nP show assoc user=$USER format=Account,Partition,QOS   # what you hold
```
