---
name: bodhi-software
description: How software is provided on the Bodhi cluster — the module tree of ~137 preinstalled bioinformatics tools, Singularity containers, and user-level pixi/uv environments — and the order to try them in. Use before installing, building, or compiling anything, when a command is not found, or when choosing how to pin a tool version for a pipeline.
---

# Getting a tool

**Look before you build.** The cluster already ships around 137 packages, and
building `bcftools` from source or pip-installing a bioinformatics tool that
is one `module load` away wastes an hour and produces a less reproducible
result.

The order is: **module → container → pixi/uv**.

## 1. Modules

Environment Modules 5.3.0 (Tcl, not Lmod — there is no `module spider`):

```bash
module avail                          # the whole catalogue
module avail 2>&1 | grep -i star      # avail writes to STDERR; grep needs 2>&1
module whatis samtools                # what a name resolves to
module show samtools                  # what it puts on PATH, and its own deps
module load samtools/1.22.1
module list
module purge                          # start clean
```

The tree at `/cluster/software/modules-sw` covers most of what a genomics
pipeline needs — aligners (`bwa`, `bowtie2`, `STAR`, `minimap2`, `hisat2`),
`samtools`/`bcftools`/`htslib`/`bedtools`/`bedops`, `cellranger` and friends,
`picard`, `ncbi-blast`, `salmon`, `kallisto`, `R` (4.3.3, 4.5.1, 4.5.2),
`java` (8 through 25), `plink`, `sratoolkit`.

**Pin the version.** `module load STAR` takes whatever carries `(default)`
today, and defaults move — `module load STAR/2.7.11b` is what makes a run
reproducible next year. Modules also load their own dependencies (`samtools`
pulls in `htslib`), so do not hand-assemble a stack that `module show` already
describes.

**Load inside the job, not just the login shell.** A module loaded in your
session is an environment change; `sbatch` starts from a fresh login shell and
will not have it. Put the `module load` lines in the job script.

## 2. Containers

```bash
singularity exec /path/to/image.sif command ...
singularity exec --bind /beevol:/beevol image.sif command ...
```

Reach for this when a tool is not in the module tree and comes with an
official image, or when a pipeline pins one. Bind `/beevol` explicitly if the
tool needs to see cluster paths.

## 3. pixi and uv

For everything left over, and for project-local environments that belong to a
repository rather than to the cluster:

```bash
pixi add samtools                    # project env, recorded in pixi.toml
pixi global install jq               # a small tool you want on PATH everywhere
uv venv && uv pip install ...        # Python projects
```

Both live under `$HOME`, which is shared across every node, so an environment
built once in a session works in every allocation without reinstalling.

**Never `pip install` into the system Python.** It is not writable, and
`--user` puts packages on a path every job inherits, which turns one project's
pin into every project's problem. Use a project environment.

## Building from source

Only after checking `module avail`. If you do build, it is real compute —
give it its own allocation rather than running it in the session shell (see
`bodhi-compute`), install into `$HOME` or a project prefix, and write down in
the project what was built and why the module tree was not enough.
