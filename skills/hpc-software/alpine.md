# Software on Alpine (CU Boulder / CURC)

Surveyed live **2026-08-27**; `module --version` says which module system
you actually have if in doubt.

## Lmod, hierarchical

Alpine runs **Lmod** (Lua modules), and the tree is hierarchical: loading a
compiler (`gcc`, `intel`, `aocc`, `nvhpc_sdk`) exposes MPI modules, which
expose MPI-dependent libraries.

```bash
module spider NAME        # searches the WHOLE tree; says what to load first
module avail              # only what is reachable from your current loads
module load gcc/14.2.0    # pin versions, as everywhere
```

`module avail` not showing a package does not mean it is absent — `spider`
is the search tool.

`StdEnv` is loaded by default and provides `curc-quota` and the Slurm tools.

## The catalogue is general HPC, not bioinformatics

Compilers, MPI, `matlab`, `R`, `anaconda`/`miniforge`, `singularity`. A
genomics tool is usually *not* a module here — go straight to pixi/uv or a
container rather than hunting the tree for it.

## Containers

```bash
module load singularity
singularity exec --bind /scratch/alpine:/scratch/alpine,/projects:/projects \
  image.sif command ...
```

## Environments and caches must not live in `$HOME`

`$HOME` is 2 GB on Alpine. Put project environments in `/projects/$USER` or
on scratch, and point the caches at scratch before the first install:

```bash
export UV_CACHE_DIR=/scratch/alpine/$USER/.cache/uv
export PIXI_CACHE_DIR=/scratch/alpine/$USER/.cache/pixi
export CARGO_HOME=/scratch/alpine/$USER/software/rust/cargo
export RUSTUP_HOME=/scratch/alpine/$USER/software/rust/rustup
```

(Bodhi's assumption that `$HOME` is the roomy shared place is exactly wrong
here — see `hpc-storage`.)

## Builds

Use the `acompile` partition (`--qos=compile`, 12h, ≤8 CPUs, 4 jobs) rather
than the session shell or a general allocation.
