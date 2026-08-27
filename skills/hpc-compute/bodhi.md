# Compute on Bodhi

## Choosing a partition

```bash
sinfo -o "%20P %5a %10l %6D %6t %N"          # what exists, and what's idle
```

Pick the partition the work belongs to — `rna` for rnabioco work (6+ nodes,
usually several idle), `normal` as the general fallback, `bigmem` for
memory-heavy jobs, `gpu` for GPUs. **Never `interactive`** — it is reserved
for sinteractive sessions and is the smallest partition on the cluster
(~3 nodes). No `--qos` is needed on Bodhi unless you are reaching for `long`.

`gpu` and some other partitions restrict which accounts may submit
(`AllowAccounts=gpu_rbi,...`), so the right `-p` can still be rejected under
the default account — the fix is `-A gpu_rbi`, not a smaller request.

**`DefMemPerCPU` is 4000 MB.** Leaving `--mem` off means 4G per CPU, not
unlimited.

The `interactive` partition allows only 4 concurrent jobs per user. `You
already have 4/4 interactive jobs` means that one partition is full, not the
cluster — launch the session elsewhere with `-p rna` rather than cancelling
somebody's session to free a slot.

## The monthly maintenance window

Bodhi takes a maintenance reservation roughly once a month, and it covers
**every node on the cluster**. Check before requesting anything long:

```bash
scontrol show reservation
# ReservationName=monthly-maint StartTime=2026-08-27T06:00:00
#   EndTime=2026-08-28T06:00:00 Duration=1-00:00:00
#   Nodes=compgpu[01-03],compute[00-21] NodeCnt=25
#   Flags=MAINT,IGNORE_JOBS,SPEC_NODES,ALL_NODES
#   Users=root  State=INACTIVE
```

`State=INACTIVE` means it has not started yet; `ACTIVE` means it is on and the
nodes are gone. `Users=root` means it is not a reservation you can submit
into. `ALL_NODES` is the part that matters — there is nowhere else to go.

With the window 21h49m away:

```console
$ srun --test-only -p rna -c 4 --mem 8G -t 21:00:00 -- true
srun: Job 245072 to start at 2026-08-26T08:10:20 ...      # starts now

$ srun --test-only -p rna -c 4 --mem 8G -t 22:00:00 -- true
srun: Job 245073 to start at 2026-08-28T06:00:00 ...      # +46 hours
```

One extra hour of requested walltime cost two days of waiting. **Size `-t`
to fit in the gap.** How long is the gap:

```bash
scontrol show reservation 2>/dev/null |
  sed -n 's/.*ReservationName=\([^ ]*\) StartTime=\([^ ]*\) EndTime=\([^ ]*\).*/\1 \2 \3/p' |
  while read -r name start end; do
    s=$(date -d "$start" +%s) now=$(date +%s)
    ((s > now)) && printf '%s starts in %dh%02dm\n' \
      "$name" $(((s - now) / 3600)) $((((s - now) % 3600) / 60))
  done
# monthly-maint starts in 21h49m
```

`IGNORE_JOBS` means the reservation was allowed to be created over jobs that
were already running, so those are not killed up front — but they do not
survive the window either. **Nothing running is safe across it**, sinteractive
sessions included: a session whose walltime crosses the start time will be cut
short, so launch one that ends before the window rather than one that reaches
past it.
