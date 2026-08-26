---
name: bodhi-storage
description: Where data lives on the Bodhi cluster and where to write it — the shared /beevol BeeGFS filesystem versus node-local /tmp, which one intermediates belong on, and how to check space without hanging the session. Use before writing large output, staging inputs, choosing a working directory for a job, or when a job is slow at I/O or the filesystem is full.
---

# Where does it go?

## One shared filesystem, and one local disk

```bash
df -h /beevol /tmp
# beegfs_nodev              839T  700T  139T  84%  /beevol      <- shared, everyone
# /dev/mapper/system-root   423G   22G  401G   6%  /            <- this node only
```

`/beevol` is a single BeeGFS mount and the only thing shared between nodes:

| Path | What it is |
|---|---|
| `/beevol/home/$USER` | Your home. Code, environments, results worth keeping. |
| `/beevol/data` | Shared reference and project data. |
| `/beevol/illumina` | Sequencer output — `runs/`, `data/`. Read from it, don't write to it. |

`/tmp` is the compute node's own 423G disk (Slurm's `TmpFS`), and `/dev/shm`
is a 377G tmpfs — RAM, so anything you put there counts against your job's
`--mem` and disappears with the allocation.

**The filesystem is 84% full and it is shared with everyone.** Space you free
is space someone else's run does not fail for.

## The rule

**Read inputs from `/beevol`, write scratch to node-local `/tmp`, copy the
results back.** A pipeline that streams thousands of small writes to BeeGFS
is slow for you and slow for everyone else on the cluster; the same work
against local disk is not.

Slurm does not hand out a private temp directory here — `TMPDIR` is plain
`/tmp` and `SLURM_TMPDIR` is unset — so make your own and clean it up,
because nothing else will:

```bash
#!/usr/bin/env bash
set -euo pipefail
work=/tmp/$USER-$SLURM_JOB_ID
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT          # runs on success, failure, and scancel

samtools sort -@ 8 -T "$work"/sort -o "$work"/out.bam /beevol/data/in.bam
cp "$work"/out.bam /beevol/home/$USER/results/
```

The `trap` matters: `/tmp` on a shared node already has a couple of thousand
entries, and an uncleaned job directory sits there until someone notices.

Keep the *final* artifacts on `/beevol` — `/tmp` is node-local, so the next
job in the pipeline probably lands somewhere else and cannot see it.

## Checking space

`df` is instant. **`du` on a home directory is not** — BeeGFS has to walk
every file, and `du -sh /beevol/home/$USER` can run for many minutes and give
you nothing to show for it. Point it at a subdirectory you actually suspect,
give it a timeout, and run it in an allocation rather than in the session:

```bash
df -h /beevol                                   # always safe
du -sh --max-depth=1 ~/devel 2>/dev/null        # one level, one subtree
```

For finding what to delete, target the big and the old rather than
summarising everything:

```bash
find ~/ -xdev -type f -size +5G -printf '%s\t%p\n' 2>/dev/null | sort -rn | head
```

## Your own quota

`df` shows the filesystem; it says nothing about your personal limit, which is
what actually stops your writes:

```bash
sinteractive --check-quota
# OVER QUOTA: 30.2T of 30T used (100.7%), over by 204.8G
# Quota OK: 24.1T of 30T used (80.3%)

sinteractive --check-quota --json      # same, machine-readable
```

A session shows a red `OVER QUOTA` notice on a line below its status bar
while this holds, refreshed every ten minutes.

**After deleting anything on the user's behalf, run `--check-quota`.** It
re-checks immediately and pushes the result to every open session, so the
warning clears within a tick instead of at the end of the poll interval. The
user asked for the space back; leaving a stale warning on their screen makes
it look like the deletion did not work.

Being over quota is not an error in the check — the command exits 0 either
way, and exits 1 only when the quota genuinely cannot be read.

## Before writing something large

Estimate the output, check `df -h /beevol` **and** `sinteractive
--check-quota`, and say so if the run would take a visible bite out of either.
Ask the user before writing hundreds of gigabytes to shared storage — on a
filesystem this full that is a decision about other people's work, not just
theirs. If they are already over quota, say so before starting rather than
after the writes fail.

Sizing an allocation for the job that does the writing is the `bodhi-compute`
skill; finding out which partition you may use is `slurm-discovery`.
