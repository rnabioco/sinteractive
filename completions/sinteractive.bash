# Bash completion for sinteractive.
#
# Completes option flags and, after --attach/--status/--refresh/--cancel/
# --ensure, the job ids and names of running sessions. Targets are read from the state files
# at ~/.cache/sinteractive/*.json (written by each running session, removed at
# teardown) instead of squeue, so completion stays instant even when the
# scheduler is slow.
#
# Installed by `make install` into the bash-completion user/system dirs; or
# source this file directly from ~/.bashrc.

_sinteractive() {
  local cur prev
  # Reset first: bash does not clear COMPREPLY between invocations, so an
  # early return would otherwise offer the previous completion's words.
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD - 1]}"

  case "$prev" in
  -a | --attach | --status | --refresh | --cancel | --ensure)
    local dir="${HOME}/.cache/sinteractive" targets='' f id name
    for f in "$dir"/*.json; do
      [[ -e "$f" ]] || continue
      id="${f##*/}"
      targets+=" ${id%.json}"
      # State files are single-line JSON; pull "name" out with sed (null and
      # missing names produce no output). The pattern is greedy, so it matches
      # the LAST "name": on the line — never add a key ending in `name`
      # (job_name, node_name) after this one or completion silently breaks.
      name=$(sed -n 's/.*"name":"\([^"]*\)".*/\1/p' "$f" 2>/dev/null)
      [[ -n "$name" ]] && targets+=" ${name}"
    done
    # mapfile rather than COMPREPLY=($(...)): the array form re-splits the
    # results on IFS and glob-expands them, which a session named "*" would
    # notice. Also what shellcheck asks for (SC2207).
    mapfile -t COMPREPLY < <(compgen -W "$targets" -- "$cur")
    return
    ;;
  # Options whose value can't be guessed: complete nothing (not filenames).
  -n | --name | --time | -t | -j | --threads | -m | --node | --partition)
    return
    ;;
  esac

  if [[ "$cur" == -* ]]; then
    mapfile -t COMPREPLY < <(compgen -W '
      --name --time --threads --node --partition --mouse --no-mouse
      --attach --list --status --refresh --cancel --detach --json
      --ensure --agent-context
      --help --version
    ' -- "$cur")
  fi
}

complete -F _sinteractive sinteractive
