---
name: hpc-software
description: How software is provided on the Bodhi and Alpine (CU Boulder/CURC) clusters — module trees (Tcl modules on Bodhi, Lmod on Alpine), Singularity containers, and user-level pixi/uv environments — and the order to try them in. Use before installing, building, or compiling anything, when a command is not found, or when choosing how to pin a tool version for a pipeline.
---

# Getting a tool

**Look before you build.** Building `bcftools` from source or pip-installing
a tool that is one `module load` away wastes an hour and produces a less
reproducible result.

The order is: **module → container → pixi/uv**. The module systems and
catalogues differ per cluster — detect which you are on and read **only that
cluster's file** in this skill's directory:

```bash
[ -d /scratch/alpine ] && echo alpine || { [ -d /beevol ] && echo bodhi; }
```

- **Alpine** (CU Boulder / CURC) → read `alpine.md` next to this SKILL.md
- **Bodhi** → read `bodhi.md` next to this SKILL.md

## What holds on both

**Pin the version.** `module load STAR` takes whatever carries the default
today, and defaults move — `module load STAR/2.7.11b` is what makes a run
reproducible next year. Modules also load their own dependencies, so do not
hand-assemble a stack that `module show NAME` already describes.

**Load inside the job, not just the login shell.** A module loaded in your
session is an environment change; `sbatch` starts from a fresh login shell
and will not have it. Put the `module load` lines in the job script.

**Containers** are the reach when a tool is not in the module tree and comes
with an official image, or when a pipeline pins one:

```bash
singularity exec image.sif command ...
singularity exec --bind /path:/path image.sif command ...
```

Bind the cluster paths the tool needs to see explicitly — which paths, and
whether `singularity` itself needs a `module load` first, is in the cluster
file.

**pixi and uv** cover everything left over, and project-local environments
that belong to a repository rather than to the cluster:

```bash
pixi add samtools                    # project env, recorded in pixi.toml
pixi global install jq               # a small tool you want on PATH everywhere
uv venv && uv pip install ...        # Python projects
```

Environments are shared across every node either way, so one built in a
session works in every allocation — but *where* they and their caches may
live differs per cluster (Alpine's 2 GB `$HOME` forbids it; see the cluster
file and `hpc-storage`).

**Never `pip install` into the system Python.** It is not writable, and
`--user` puts packages on a path every job inherits, which turns one
project's pin into every project's problem. Use a project environment.

## Building from source

Only after checking the module tree. If you do build, it is real compute —
give it its own allocation rather than running it in the session shell (see
`hpc-compute`; Alpine has a dedicated `acompile` partition), install into a
project prefix, and write down in the project what was built and why the
module tree was not enough.
