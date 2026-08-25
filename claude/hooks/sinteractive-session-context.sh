#!/usr/bin/env bash
#
# Claude Code SessionStart hook — tell the agent which sinteractive session it
# is running inside, how big that allocation is, how much walltime is left,
# and that the session is an orchestration shell rather than a compute target.
#
# Wire it up in ~/.claude/settings.json:
#
#   "hooks": {
#     "SessionStart": [
#       { "hooks": [ { "type": "command", "timeout": 10,
#           "command": "bash ~/.claude/hooks/sinteractive-session-context.sh" } ] }
#     ]
#   }
#
# Claude Code adds a SessionStart hook's plain stdout to the agent's context,
# so no JSON encoding is needed here — which keeps the hook free of a jq or
# python3 dependency it would otherwise carry onto every cluster node.
#
# Always exits 0. Outside a session `sinteractive --agent-context` exits 1,
# and a nonzero hook puts an error notice in the transcript; "not in an
# allocation" is a fact about where you are, not a failure. Same for
# sinteractive not being installed at all.

set -u

command -v sinteractive >/dev/null 2>&1 || exit 0

sinteractive --agent-context 2>/dev/null || true
exit 0
