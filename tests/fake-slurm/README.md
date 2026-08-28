# fake-slurm

Executable shims that stand in for the Slurm client tools (and `ssh`) so the
integration tests can exercise `sinteractive` without a cluster. Tests put this
directory first on `PATH` and point `FAKE_SLURM_DIR` at a fixture directory;
`crates/sint/tests/common/mod.rs` does both.

Every shim appends its invocation to `$FAKE_SLURM_DIR/calls.log`, one call per
line: the tool name, then each argument, tab-separated. Anything a shim does
not model is an error (stderr + exit 1) rather than a plausible-looking blank,
so a test that drifts from the modelled surface fails loudly.

## Fixture directory

| File           | Meaning                                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------------------------ |
| `jobs.tsv`     | The queue: one job per line, 13 tab-separated columns (below). Missing = empty queue.                        |
| `next_id`      | Id the next `sbatch` hands out (default `1000`); bumped on each submit.                                      |
| `sbatch.fail`  | When present, `sbatch` prints its contents to stderr and exits 1.                                            |
| `sbatch.last`  | Written by `sbatch`: the submitted script and its arguments, one per line.                                   |
| `reservations` | Output of `scontrol show reservation -o` (one reservation per line, `ReservationName=… StartTime=… …`).       |
| `sacct`        | Output of `sacct` (any arguments).                                                                           |
| `qos`          | Output of `sacctmgr` (any arguments), e.g. `sacctmgr show qos … --parsable2 --noheader`.                     |
| `sinfo`        | Output of `sinfo` (any arguments).                                                                           |
| `calls.log`    | Appended by every shim (see above).                                                                          |

### `jobs.tsv` columns

```
1 job_id   2 comment   3 node   4 partition   5 elapsed   6 time_limit
7 end_time   8 cpus   9 mem   10 tres   11 state   12 reason   13 start_time
```

Values are the raw strings squeue would print: `elapsed` like `1:23:45`,
`time_limit` like `8:00:00`, `end_time`/`start_time` like
`2026-01-01T08:00:00` (or `N/A`/`Unknown`), `mem` like `8G`/`4000M`, `tres`
like `gres:gpu:1` or `N/A`, `state` `RUNNING`/`PENDING`/…, `reason` `None` or
a pend reason. A session is a job whose comment is `sinteractive` or
`sinteractive:NAME`. An empty comment prints as `(null)` through `%k`, exactly
as real squeue does.

## Shims

`squeue` — filters `jobs.tsv` and formats it.
Filters: `--me`/`-u USER` (every job is yours), `--jobs ID[,ID]`/`-j`,
`--states A,B`/`-t`/`--state` (case-insensitive, `all` = no filter),
`--partition P`/`-p`. `--noheader`/`-h` drops the header line. Output is
either `-o FORMAT`/`--format` with codes `%i %k %N %P %M %l %e %C %m %b %T %r
%S` (plus `%j` name, `%t` compact state, `%u`, `%D`; a width like `%.10i` is
accepted and ignored; every other character, `|` included, is literal) or
`--Format FIELD[,FIELD]`/`-O` with `jobid comment batchhost nodelist partition
timeused timelimit endtime numcpus minmemory tres state statecompact reason
starttime name username`, each padded to 20 columns plus a space like the
real thing (callers `xargs`/trim it). All `--flag=value` spellings work.

`sbatch` — appends a `RUNNING` job on `fakenode01` with an empty comment and
prints `Submitted batch job N` (`--parsable`: just `N`). `--wrap CMD` is recorded
in `sbatch.last` split into words (that is how the Rust tool submits). `--partition`,
`--time`, `--cpus-per-task`, `--mem`, `--gres`, `--nodelist`, `--comment`
(short forms too) populate the row; defaults `interactive 8:00:00 2 8G N/A`.
Other options are accepted and ignored. The first bare token is the script.

`scontrol` — `update JobId=N Comment=X` rewrites the comment (`Name=`,
`TimeLimit=` accepted and ignored; unknown job → error); `show reservation
[-o]` prints `reservations`; `show config` prints `ClusterName = fake`; `show
job N` prints a one-line summary.

`scancel [flags] N…` — removes the jobs; unknown id → real error text, exit 1.

`sacct`, `sacctmgr`, `sinfo` — print the fixture file of the same name
(`qos` for sacctmgr), or nothing.

`srun [opts] [--] CMD…` — execs `CMD…` locally.

`ssh [opts] HOST CMD…` — runs `CMD…` locally through `bash -c` with
`FAKE_SSH_HOST=HOST` exported; a login with no command is an error.

## Seeding by hand

```sh
export FAKE_SLURM_DIR=$(mktemp -d)
printf '147845\tsinteractive:web\tnode01\tinteractive\t1:02:03\t8:00:00\t2026-01-01T08:00:00\t4\t16G\tN/A\tRUNNING\tNone\t2026-01-01T00:00:00\n' \
  > "$FAKE_SLURM_DIR/jobs.tsv"
PATH=$PWD/tests/fake-slurm:$PATH squeue --me --noheader -o '%i|%k|%N'
```
