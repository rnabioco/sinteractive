#!/usr/bin/env bash
# Shared helpers for the fake Slurm shims. Sourced, not executed.
#
# The fixture directory is $FAKE_SLURM_DIR; jobs.tsv inside it is the queue,
# one job per line with these tab-separated columns (1-based, for awk):
#
#   1 job_id  2 comment  3 node  4 partition  5 elapsed  6 time_limit
#   7 end_time  8 cpus  9 mem  10 tres  11 state  12 reason  13 start_time
#
# See README.md for the full contract.

set -u

: "${FAKE_SLURM_DIR:?FAKE_SLURM_DIR must point at a fixture directory}"
JOBS="${FAKE_SLURM_DIR}/jobs.tsv"

# Append this invocation to calls.log: the shim name, then every argument,
# tab-separated, one call per line.
log_call() {
  local name=$1
  shift
  {
    printf '%s' "$name"
    local a
    for a in "$@"; do printf '\t%s' "$a"; done
    printf '\n'
  } >>"${FAKE_SLURM_DIR}/calls.log"
}

# Echo the jobs.tsv rows (nothing when the file is absent).
read_jobs() {
  [[ -f "$JOBS" ]] && cat "$JOBS"
  return 0
}

# Rewrite jobs.tsv atomically from stdin.
write_jobs() {
  local tmp
  tmp=$(mktemp "${JOBS}.XXXXXX")
  cat >"$tmp"
  mv "$tmp" "$JOBS"
}

# Does job $1 exist?
job_exists() {
  [[ -n "$(read_jobs | awk -F'\t' -v id="$1" '$1 == id { print 1; exit }')" ]]
}

# Print the contents of fixture file $1 when it exists; nothing otherwise.
cat_fixture() {
  [[ -f "${FAKE_SLURM_DIR}/$1" ]] && cat "${FAKE_SLURM_DIR}/$1"
  return 0
}

die() {
  echo 1>&2 "$@"
  exit 1
}
