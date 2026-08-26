---
name: slurm-discovery
description: Find out what a Slurm cluster actually offers you — which partitions exist and how big they are, which accounts and QOS you hold, and which combinations you are allowed to submit. Use when choosing a partition or QOS, when sizing a job against the limits, when a submission is rejected, or when a job sits PENDING and the reason is unclear.
---

# What can I actually run here?

Ask the cluster rather than assuming. A handful of commands answer almost
everything, none costs more than a scheduler round-trip, and the answers that
matter are worth writing down once instead of rediscovering every session.

## Check the cached map first

Partitions, accounts and QOS change on the order of months. Once the survey
below has run, its answers live in a file — **read that before running
anything**.

The file is keyed by cluster, because the same `$HOME` is often mounted on
more than one and a map from the wrong one is worse than none:

```bash
cluster=$(scontrol show config | sed -n 's/^ClusterName *= *//p' | tr -d '[:space:]')
map=~/.cache/sinteractive/slurm-map-${cluster:-unknown}.md
cat "$map"                                  # nothing? build it, below
```

Build or refresh it in one go. `sinteractive` only ever removes files named
after its own job ids, so this one is safe alongside them:

```bash
mkdir -p ~/.cache/sinteractive
{
  echo "# Slurm map for $USER on ${cluster:-unknown}"
  echo "# Generated $(date -Is). Rebuild when a partition or account changes."
  echo; echo '## Partitions'
  sinfo -o "%20P %5a %10l %10L %6D %8c %10m %12G"
  echo; echo '## My associations (account|partition|QOS|...)'
  sacctmgr -nP show assoc user="$USER" \
    format=Account,Partition,QOS,MaxJobs,MaxSubmit,GrpTRES,MaxTRES,MaxWall
  echo; echo "## My default account"
  sacctmgr -nP show user "$USER" format=User,DefaultAccount
  echo; echo '## QOS limits'
  sacctmgr -nP show qos \
    format=Name,Priority,MaxWall,MaxTRESPU,MaxJobsPU,MaxSubmitJobsPU,GrpTRES,Flags
  echo; echo '## Partition access (AllowAccounts / AllowQos)'
  scontrol show partition | grep -E '^PartitionName=|AllowAccounts='
} > "$map"
```

Rebuild it when something stops matching — a partition you were told about is
missing, or an account is rejected that the map says you hold — and when the
`date -Is` in the header is more than a month or two old.

**Cache the map, never the weather.** Node states, queue depth and who is
running what change by the minute, and a cached `idle` is a lie within
minutes. That is why the `sinfo` line above drops the state column the survey
below keeps: what belongs in the file is the structure — which partitions
exist, how big they are, what you may ask for, and the limits on it. Anything
about right now gets run live, every time.

Maintenance reservations are the tempting exception, and they are weather too:
they recur monthly but each one has a date, and a stale window in a file is
exactly the sort of thing that gets trusted. `scontrol show reservation` is
one call — run it, do not cache it. See `bodhi-compute` for what to do with
the answer.

## The survey

**What partitions exist, and how big are they?**

```bash
sinfo -o "%20P %5a %10l %10L %6D %6t %8c %10m %12G %N"
#        PARTITION AVAIL TIMELIMIT DEFAULTTIME NODES STATE CPUS MEMORY GRES NODELIST
```

`TIMELIMIT` is the ceiling, `DEFAULTTIME` is what you get by leaving `-t`
off — usually much shorter, and a common cause of a job dying early. `MEMORY`
is per node in MB, and a trailing `+` means the nodes in that row differ.

**What do I hold?**

```bash
sacctmgr -nP show assoc user=$USER \
  format=Account,Partition,QOS,MaxJobs,MaxSubmit,GrpTRES,MaxTRES,MaxWall
# rbi||high,long,normal,positron|||||
# gpu_rbi||high,long,normal|||||

sacctmgr -nP show user $USER format=User,DefaultAccount
# jhessel|rbi
```

One row per account. The QOS column is what that account may request; empty
limit columns mean the limit comes from the QOS, not the association.

**What does the partition allow?**

```bash
scontrol show partition rna
#   AllowGroups=ALL AllowAccounts=rbi AllowQos=long,normal
#   DefaultTime=04:00:00 MaxTime=UNLIMITED DefMemPerNode=12000
```

## The rule

**You can submit to a partition when your account is in its `AllowAccounts`
and the QOS you ask for is in both its `AllowQos` and your association's QOS
list.** Both halves have to hold. Neither is implied by the other, and the
error you get for failing either is the same unhelpful "invalid account or
partition".

The default account is the trap. On Bodhi, `gpu` has
`AllowAccounts=gpu_rbi,gpu_devbio,gpu_scb`, so a default account of `rbi` is
rejected there however many GPUs are idle — the fix is `-A gpu_rbi`, not a
smaller request:

```bash
srun -p gpu -A gpu_rbi --gres=gpu:1 -c 8 --mem 32G -t 2:00:00 \
  -J probe --comment=probe -- nvidia-smi -L
```

## Reading the limits

```bash
sacctmgr -nP show qos \
  format=Name,Priority,MaxWall,MaxTRESPU,MaxJobsPU,MaxSubmitJobsPU,GrpTRES,Flags
# normal|25|3-00:00:00||500|2000||DenyOnLimit
# long|50|7-00:00:00|cpu=128|12|50|cpu=156|OverPartQOS
# interactive|50|12:00:00|cpu=16,mem=8G|4|3||DenyOnLimit,OverPartQOS
```

- `MaxWall` caps a single job. Asking for more is rejected outright, not
  trimmed — `long` is how you get past the `normal` QOS's ceiling.
- `MaxTRESPU` / `MaxJobsPU` are **per user**, and `GrpTRES` is across everyone
  on that QOS. A job can be legal on its own and still queue because your
  other jobs are already holding the budget.
- `OverPartQOS` means the QOS limit wins over the partition's; without it the
  tighter of the two applies.
- `DenyOnLimit` rejects an over-limit job at submit time instead of queueing
  it forever. Its absence is why some requests vanish into `PENDING`.

## What is free right now

```bash
sinfo -p rna -o "%6t %6D %8c %10m %N"      # idle vs mix vs alloc, by node
squeue -p rna -o "%.10i %.10u %.10M %.6C %R" | head
```

`idle` nodes are whole and free; `mix` has room but is shared. Sizing a
request to what is actually idle is the difference between starting now and
starting tomorrow.

## When a job will not run

```bash
squeue --me -o "%.10i %.20j %.10T %r"      # %r is the reason
```

The reason names the wall you hit:

| Reason | Meaning |
|---|---|
| `Resources` | The request is legal; the nodes are busy. Wait, or shrink it. |
| `Priority` | Legal, but others are ahead. Check `sshare -U` for fairshare. |
| `QOSMaxWallDurationPerJobLimit` | `-t` exceeds the QOS `MaxWall`. Ask for a longer QOS. |
| `QOSMaxCpuPerUserLimit`, `AssocMaxJobsLimit` | Your own running jobs are holding the budget. |
| `PartitionTimeLimit`, `PartitionConfig` | The request cannot fit the partition at all. |
| `ReqNodeNotAvail` | Named nodes are down or drained — check `sinfo -R`. |
| `ReqNodeNotAvail, Reserved for maintenance` | `-t` reaches past a maintenance window, so the job is deferred until after it. Shorten it — see `bodhi-compute`. |

For a job already rejected at submit, re-run with `--test-only` to get the
verdict without queueing anything:

```bash
srun --test-only -p rna -A rbi -c 8 --mem 32G -t 1:00:00 -- true
```

## Then go run something

This skill is about finding out what is available. Actually placing work — in
its own allocation, named in both `-J` and `--comment`, never in the shell you
are typing in — is the `bodhi-compute` skill.
