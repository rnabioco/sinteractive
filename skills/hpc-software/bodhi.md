# Software on Bodhi

## Tcl Environment Modules

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

## The catalogue is bioinformatics-rich

The tree at `/cluster/software/modules-sw` ships around 137 packages and
covers most of what a genomics pipeline needs — aligners (`bwa`, `bowtie2`,
`STAR`, `minimap2`, `hisat2`), `samtools`/`bcftools`/`htslib`/`bedtools`/
`bedops`, `cellranger` and friends, `picard`, `ncbi-blast`, `salmon`,
`kallisto`, `R` (4.3.3, 4.5.1, 4.5.2), `java` (8 through 25), `plink`,
`sratoolkit`. Most of a genomics pipeline is a `module load` away — check
here before reaching for pixi or a container.

## Containers

`singularity` is on `PATH` without a module load. Bind `/beevol` explicitly
if the tool needs to see cluster paths:

```bash
singularity exec --bind /beevol:/beevol image.sif command ...
```

## Environments

pixi and uv environments live under `$HOME`, which is shared across every
node and roomy enough for them here — an environment built once in a session
works in every allocation without reinstalling.
