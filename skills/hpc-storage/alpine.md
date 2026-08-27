# Storage on Alpine (CU Boulder / CURC)

## Three tiers with three different jobs

```bash
df -h /home/$USER /projects/$USER /scratch/alpine/$USER
# isilon...:/ifs/curc/home       2.0G   <- home: tiny NFS
# isilon...:/ifs/curc/projects   250G   <- projects: small NFS, backed up
# alpine1                        2.8P   <- scratch: GPFS parallel filesystem
```

| Path | Size | Backed up | What belongs there |
|---|---|---|---|
| `/home/$USER` | **2 GB** | — | Dotfiles and almost nothing else. |
| `/projects/$USER` | 250 GB | yes | Code, repos, configs, environments, results worth keeping. |
| `/scratch/alpine/$USER` | ~10 TB | **no** | **All work.** Job working dirs, intermediates, large data. |
| `/pl/active/<alloc>` | per group | snapshots | PetaLibrary — paid group allocations for long-term data. |

**Nothing goes in `$HOME`.** Two gigabytes fills the moment a tool starts
caching there, and a full home breaks logins and every job that touches it.
Tool caches are the usual culprit — point them at scratch before installing
anything:

```bash
export UV_CACHE_DIR=/scratch/alpine/$USER/.cache/uv
export PIXI_CACHE_DIR=/scratch/alpine/$USER/.cache/pixi
export CARGO_HOME=/scratch/alpine/$USER/software/rust/cargo
export RUSTUP_HOME=/scratch/alpine/$USER/software/rust/rustup
```

**All compute work runs out of `/scratch/alpine/$USER`.** It is the GPFS
parallel filesystem — scratch *is* the fast storage here, built for job I/O.
It is not backed up, and **files untouched for ~90 days are eligible for
purge**, so when a run finishes, copy the results worth keeping to
`/projects/$USER` (small things) or a PetaLibrary allocation (large things)
and treat what remains on scratch as disposable.

`/projects/$USER` is small and backed up: the right home for repositories,
notebooks, environments, and final small outputs — never for the working set
of a running pipeline.

Node-local `/tmp` on a compute node is only ~63 GB — fine for a tool's small
temp files, too small for genomics intermediates. The working directory for
a job is scratch.

## Checking space

```bash
curc-quota
# shows used/avail/limit for home, projects, scratch, and every
# /pl/active allocation your groups hold, in one call
```

`sinteractive --check-quota` is a Bodhi feature — on Alpine it prints no
quota notice at all (the Bodhi quota daemons don't exist here). `curc-quota`
is the tool, and it is on `PATH` via the default `StdEnv` module.

## Keeping this file honest

The numbers here were scraped from a live compute node on **2026-08-27**.
Quota sizes and the purge window are policy, not filesystem facts — CURC
adjusts them. When something looks off, re-run the survey and fix this file
rather than trusting it:

```bash
scontrol show config | grep ClusterName          # which cluster this is
df -h /home/$USER /projects/$USER /scratch/alpine/$USER   # tiers, sizes, mounts
curc-quota                                       # authoritative quotas, incl. /pl
df -h /tmp                                       # node-local disk (ran: 63G on acpu)
```
