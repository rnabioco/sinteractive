---
name: hpc-storage
description: Where data lives on the Bodhi and Alpine (CU Boulder/CURC) clusters and where to write it — Bodhi's single shared /beevol BeeGFS versus Alpine's tiered /home (tiny), /projects (small, backed up), /scratch/alpine (huge, purged) layout, which one intermediates belong on, and how to check space without hanging the session. Use before writing large output, staging inputs, choosing a working directory for a job, or when a job is slow at I/O or the filesystem is full.
---

# Where does it go?

Two clusters share this skill and their filesystems are nothing alike — the
right place to write on one is a mistake on the other. Find out which you
are on, then read **only that cluster's file** in this skill's directory;
the other is noise:

```bash
[ -d /scratch/alpine ] && echo alpine || { [ -d /beevol ] && echo bodhi; }
```

- **Alpine** (CU Boulder / CURC) → read `alpine.md` next to this SKILL.md
- **Bodhi** → read `bodhi.md` next to this SKILL.md

## What holds on both

**`df` is instant; `du` on a home directory is not.** A networked filesystem
walks every file, and `du -sh` over a home can run for many minutes with
nothing to show for it. Point it at a subdirectory you actually suspect
(`du -sh --max-depth=1 DIR`), and run big scans in an allocation, not the
session. For finding what to delete, target the big and the old:

```bash
find DIR -xdev -type f -size +5G -printf '%s\t%p\n' 2>/dev/null | sort -rn | head
```

**Before writing something large**: estimate the output, check the
filesystem and your quota (each cluster's file says how), and say so if the
run would take a visible bite out of either. Ask the user before writing
hundreds of gigabytes to shared storage — that is a decision about other
people's work, not just theirs. If they are already over quota, say so
before starting rather than after the writes fail.

**Final artifacts go on storage every node can see** — anything staged on a
node-local disk vanishes from the next job's point of view.

Sizing an allocation for the job that does the writing is the `hpc-compute`
skill; finding out which partition you may use is `slurm-discovery`.
