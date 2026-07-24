# Bash completion for sinteractive.
#
# Completes option flags and, after --attach/--status/--cancel, the job ids
# and names of running sessions. Targets are read from the state files at
# ~/.cache/sinteractive/*.json (written by each running session, removed at
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
  -a | --attach | --status | --cancel)
    local dir="${HOME}/.cache/sinteractive" targets='' f id name
    for f in "$dir"/*.json; do
      [[ -e "$f" ]] || continue
      id="${f##*/}"
      targets+=" ${id%.json}"
      # State files are single-line JSON; pull "name" out with sed (null and
      # missing names produce no output).
      name=$(sed -n 's/.*"name":"\([^"]*\)".*/\1/p' "$f" 2>/dev/null)
      [[ -n "$name" ]] && targets+=" ${name}"
    done
    COMPREPLY=($(compgen -W "$targets" -- "$cur"))
    return
    ;;
  # Options whose value can't be guessed: complete nothing (not filenames).
  -n | --name | --time | -t | -j | --threads | -m | --node | --partition)
    return
    ;;
  esac

  if [[ "$cur" == -* ]]; then
    COMPREPLY=($(compgen -W '
      --name --time --threads --node --partition --mouse --no-mouse
      --attach --list --status --cancel --detach --json
      --help --version
    ' -- "$cur"))
  fi
}

complete -F _sinteractive sinteractive
