#!/usr/bin/env bash
#
# Claude Code UserPromptSubmit hook — warn the agent when the sinteractive
# session it is running inside is close to its walltime, so it does not start
# work the session will not survive.
#
# Wire it up in ~/.claude/settings.json:
#
#   "hooks": {
#     "UserPromptSubmit": [
#       { "hooks": [ { "type": "command", "timeout": 10,
#           "command": "bash ~/.claude/hooks/sinteractive-walltime-guard.sh" } ] }
#     ]
#   }
#
# Silent above the threshold, which is the common case — no output, no
# scheduler traffic. Set SINTERACTIVE_AGENT_WARN to the number of seconds
# remaining at which it should start speaking up (default 1800). Claude Code
# adds a UserPromptSubmit hook's plain stdout to the agent's context.
#
# UserPromptSubmit rather than PreToolUse: once per turn is the right cadence
# for "can this finish?", and it keeps the check off the path of every single
# Bash call. It does mean work already in flight cannot be warned about — only
# a longer walltime fixes that, and on most clusters an ordinary user can only
# reduce a job's TimeLimit, not raise it.
#
# Always exits 0: a nonzero hook puts an error notice in the transcript, and
# every reason this can bail (not in a session, no state file, scheduler
# unreachable) is a normal condition rather than a failure.

set -u

[[ -n "${SINTERACTIVE_JOB_ID:-}" ]] || exit 0

warn_at="${SINTERACTIVE_AGENT_WARN:-1800}"
state="${HOME}/.cache/sinteractive/${SINTERACTIVE_JOB_ID}.json"
now=$(date +%s)

# Pull one integer field out of a single-line JSON object. Non-numeric values
# (notably null, which --status emits for a job with no scheduled end) yield
# an empty string, which every caller below treats as "don't know".
json_int() {
  local key=$1 src=$2 val
  val=$(sed -n "s/.*\"${key}\":\([0-9-]\{1,\}\).*/\1/p" <<<"$src")
  [[ "$val" =~ ^-?[0-9]+$ ]] && printf '%s' "$val"
}

remaining=''
end_epoch=''

# Prefer the cached state file: the session's own status loop refreshes it
# about every 30 s, having re-confirmed the deadline against Slurm immediately
# before each write. Age it exactly rather than trusting it as-is.
if [[ -r "$state" ]]; then
  snapshot=$(<"$state")
  snap_remaining=$(json_int remaining_seconds "$snapshot")
  snap_updated=$(json_int updated_epoch "$snapshot")
  if [[ -n "$snap_remaining" && -n "$snap_updated" ]]; then
    age=$((now - snap_updated))
    # Written while squeue was unreachable? No — the loop leaves the file
    # untouched in that case, so a stale file means the scheduler has been
    # unreachable for a while. Past ~2 minutes, go ask directly.
    if ((age >= 0 && age <= 120)); then
      remaining=$((snap_remaining - age))
      end_epoch=$(json_int end_epoch "$snapshot")
    fi
  fi
fi

if [[ -z "$remaining" ]]; then
  status=$(sinteractive --status --json 2>/dev/null) || exit 0
  remaining=$(json_int remaining_seconds "$status")
  end_epoch=$(json_int end_epoch "$status")
  [[ -n "$remaining" ]] || exit 0
fi

((remaining < 0)) && remaining=0
((remaining <= warn_at)) || exit 0

if ((remaining >= 3600)); then
  left="$((remaining / 3600))h $(((remaining % 3600) / 60))m"
elif ((remaining >= 60)); then
  left="$((remaining / 60))m"
else
  left="${remaining}s"
fi

ends=''
if [[ -n "$end_epoch" ]]; then
  ends=$(date -d "@${end_epoch}" '+%H:%M' 2>/dev/null) && ends=" (ends ${ends})"
fi

cat <<MSG
Walltime warning: this sinteractive session (job ${SINTERACTIVE_JOB_ID}) has
${left} left${ends}. It self-terminates ~10s before the limit, which ends this
shell and anything attached to it, including an srun you are streaming from.

Do not start work that cannot finish in that window. Long work belongs in its
own allocation with its own -t (salloc --no-shell + srun --overlap), which
survives independently — or ask the user for a fresh session.
MSG
exit 0
