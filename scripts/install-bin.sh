#!/usr/bin/env bash
# Install the sinteractive binary without pulling it out from under anything
# that is running it.
#
#   scripts/install-bin.sh BUILT_BINARY DESTDIR
#
# Every running session is a copy of this binary executing on a compute
# node (it is the zellij server), and every attached `launch`/`attach` is
# another one on a login node. On an NFS home — Alpine's — unlinking or
# renaming over the file they are executing from another node makes the
# inode go away underneath them: the next page they fault in comes back
# ESTALE and the process dies with SIGBUS. The server goes first ("Lost
# connection to the Zellij server"), then the client on the login node
# ("Bus error (core dumped)"). `install DEST` and `mv NEW DEST` both do
# that, so neither is used on DEST.
#
# Instead each build lands under its own name, DESTDIR/.sinteractive-<id>
# (<id> = first 12 hex of its sha256, so reinstalling the same build is a
# no-op), and DESTDIR/sinteractive is a relative symlink swapped into place
# atomically. std::env::current_exe() resolves through the symlink, so a
# session keeps spawning helpers from the build it started on rather than
# mixing zellij client and server versions across an upgrade.
#
# Old builds are pruned, but only ones nothing can still be running: a job
# runs the build that was current when its launch client started, which is
# before it was submitted, so everything installed since the earliest
# submit time in the user's queue is kept, plus the two newest builds from
# before it (the one that job runs, and one of slack for clock skew). With
# no squeue, or no jobs, the current build and the two before it stay.
# When in doubt, keep: the cost of a stale copy is 50 MB of disk, the cost
# of a wrong deletion is a dead session.
set -eu

src=$1
dest=$2
name=sinteractive

[ -f "$src" ] || { echo "install-bin: $src is not a file" >&2; exit 1; }
mkdir -p "$dest"

id_of() { sha256sum "$1" | cut -c1-12; }

# A pre-existing regular file is the old, in-place install; sessions are
# running that inode right now. Give it a versioned name so the symlink can
# take over its old one without dropping its last link.
if [ -f "$dest/$name" ] && [ ! -L "$dest/$name" ]; then
  old=".$name-$(id_of "$dest/$name")"
  [ -e "$dest/$old" ] || ln "$dest/$name" "$dest/$old"
fi

id=$(id_of "$src")
new=".$name-$id"
if [ ! -f "$dest/$new" ]; then
  install -m 0755 "$src" "$dest/$new.tmp.$$"
  mv -f "$dest/$new.tmp.$$" "$dest/$new"
fi

# The symlink is relative so the whole directory can move; the swap is a
# rename, so no reader ever sees a missing `sinteractive`.
ln -sfn "$new" "$dest/$name.tmp.$$"
mv -Tf "$dest/$name.tmp.$$" "$dest/$name"
echo "installed $dest/$name -> $new"

# --- prune -----------------------------------------------------------------

# Earliest submit time among this user's jobs (every user's, for a root
# install), as epoch seconds; empty when there is nothing queued or no
# scheduler to ask.
earliest_submit() {
  local who times t e min=""
  if [ "$(id -u)" = 0 ]; then who=(-a); else who=(-u "${USER:-$(id -un)}"); fi
  times=$(squeue -h "${who[@]}" -o '%V' 2>/dev/null) || return 0
  for t in $times; do
    e=$(date -d "$t" +%s 2>/dev/null) || return 0
    if [ -z "$min" ] || [ "$e" -lt "$min" ]; then min=$e; fi
  done
  printf '%s' "$min"
}

# Versioned builds other than the one just linked, newest first.
older=()
while IFS= read -r line; do
  older+=("${line#* }")
done < <(
  for f in "$dest"/."$name"-????????????; do
    [ -f "$f" ] || continue
    [ "$(basename "$f")" = "$new" ] && continue
    printf '%s %s\n' "$(stat -c %Y "$f")" "$f"
  done | sort -rn
)

since=$(earliest_submit)
keep_after=2   # builds older than $since to keep (the one in use, plus slack)
kept=0
for f in "${older[@]}"; do
  if [ -n "$since" ] && [ "$(stat -c %Y "$f")" -ge "$since" ]; then
    continue  # installed since a job was submitted: may be what it runs
  fi
  if [ "$kept" -lt "$keep_after" ]; then
    kept=$((kept + 1))
    continue
  fi
  rm -f "$f"
  echo "pruned $f"
done
