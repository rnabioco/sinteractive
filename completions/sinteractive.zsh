#compdef sinteractive

autoload -U is-at-least

_sinteractive() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" : \
'--node=[Request a specific compute node (\`--nodelist\`)]:NODE:_default' \
'-p+[Slurm partition]:PART:_default' \
'--partition=[Slurm partition]:PART:_default' \
'-t+[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'--time=[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'-j+[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'--threads=[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'-m+[Memory (\`--mem\`)]:SIZE:_default' \
'--mem=[Memory (\`--mem\`)]:SIZE:_default' \
'-n+[Tag the session with a name for easy reattach]:NAME:_default' \
'--name=[Tag the session with a name for easy reattach]:NAME:_default' \
'--status=[]::TARGET:_default' \
'--refresh=[]::TARGET:_default' \
'--ensure=[]:NAME:_default' \
'-a+[]::TARGET:_default' \
'--attach=[]::TARGET:_default' \
'--cancel=[]:TARGET:_default' \
'--mouse[Enable mouse support in the session]' \
'--no-mouse[Disable mouse support (overrides \`SINTERACTIVE_MOUSE\`)]' \
'--detach[Launch without attaching; print connection info and return]' \
'--json[Machine-readable JSON output (with \`--detach\`)]' \
'-l[]' \
'--list[]' \
'--check-quota[]' \
'--agent-context[]' \
'--install-claude[]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sinteractive_commands" \
"*::: :->sinteractive" \
&& ret=0
    case $state in
    (sinteractive)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-command-$line[1]:"
        case $line[1] in
            (launch)
_arguments "${_arguments_options[@]}" : \
'--node=[Request a specific compute node (\`--nodelist\`)]:NODE:_default' \
'-p+[Slurm partition]:PART:_default' \
'--partition=[Slurm partition]:PART:_default' \
'-t+[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'--time=[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'-j+[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'--threads=[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'-m+[Memory (\`--mem\`)]:SIZE:_default' \
'--mem=[Memory (\`--mem\`)]:SIZE:_default' \
'-n+[Tag the session with a name for easy reattach]:NAME:_default' \
'--name=[Tag the session with a name for easy reattach]:NAME:_default' \
'--mouse[Enable mouse support in the session]' \
'--no-mouse[Disable mouse support (overrides \`SINTERACTIVE_MOUSE\`)]' \
'--detach[Launch without attaching; print connection info and return]' \
'--json[Machine-readable JSON output (with \`--detach\`)]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(attach)
_arguments "${_arguments_options[@]}" : \
'--ssh[Attach over ssh -X (X11 forwarding) instead of srun --overlap]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID, NAME, or list'\''s #; with no target, your only session:_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'--full[Also show partition and elapsed/limit]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
'--refresh[Re-check the time budget against Slurm now and update the cache]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID, NAME, or list'\''s #; defaults to the current session:_default' \
&& ret=0
;;
(cancel)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
':target -- JOBID, NAME, or list'\''s #:_default' \
&& ret=0
;;
(queue)
_arguments "${_arguments_options[@]}" : \
'--all[Include every user'\''s jobs in the partitions you can see]' \
'--watch[Refresh continuously]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(monitor)
_arguments "${_arguments_options[@]}" : \
'--job=[With --once and no TARGET\: scope to this job'\''s cgroup on this host]:JOBID:_default' \
'--live[Sample over ssh at 1 Hz instead of reading the shared snapshot]' \
'--once[Print one sample and exit instead of opening the live view]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID, NAME, list'\''s #, or a hostname; defaults to the current session:_default' \
&& ret=0
;;
(quota)
_arguments "${_arguments_options[@]}" : \
'(--check --reindex)--top=[]::N:_default' \
'--json[Machine-readable JSON output]' \
'--check[]' \
'(--check)--reindex[]' \
'(--check --top --reindex)--reindex-run[Walk home and write the usage index here and now (what the \`reindex\` job runs)]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__quota_commands" \
"*::: :->quota" \
&& ret=0

    case $state in
    (quota)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-quota-command-$line[1]:"
        case $line[1] in
            (check)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(top)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
'::n -- How many directories to list:_default' \
&& ret=0
;;
(reindex)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__quota__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-quota-help-command-$line[1]:"
        case $line[1] in
            (check)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(top)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reindex)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(scratch)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__scratch_commands" \
"*::: :->scratch" \
&& ret=0

    case $state in
    (scratch)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-scratch-command-$line[1]:"
        case $line[1] in
            (new)
_arguments "${_arguments_options[@]}" : \
'--keep-for=[Hold it against \`clean\` for this long (\`30d\`, \`36h\`, \`2w\`)]:AGE:_default' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
':topic -- Letters, digits, '\''.'\'', '\''_'\'' and '\''-'\'':_default' \
&& ret=0
;;
(ls)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
'--older-than=[Only directories created more than this long ago]:AGE:_default' \
'--yes[Actually delete (the default only lists what would go)]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__scratch__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-scratch-help-command-$line[1]:"
        case $line[1] in
            (new)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(ls)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(worktree)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__worktree_commands" \
"*::: :->worktree" \
&& ret=0

    case $state in
    (worktree)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-worktree-command-$line[1]:"
        case $line[1] in
            (ls)
_arguments "${_arguments_options[@]}" : \
'--repo=[Only the linked worktrees of this repository]:PATH:_files' \
'--no-pr[Do not ask \`gh\` whether a pull request merged (squash merges then go unseen)]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prune)
_arguments "${_arguments_options[@]}" : \
'--repo=[Only the linked worktrees of this repository]:PATH:_files' \
'--older-than=[Only worktrees made more than this long ago (\`14d\`, \`36h\`, \`2w\`)]:AGE:_default' \
'--no-pr[Do not ask \`gh\` whether a pull request merged]' \
'--yes[Actually remove (the default only lists what would go)]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(audit)
_arguments "${_arguments_options[@]}" : \
'--repo=[Only the linked worktrees of this repository]:PATH:_files' \
'--every=[How long until the next audit (\`1d\`, \`12h\`, \`1w\`)]:AGE:_default' \
'--resubmit[Submit the next audit as a Slurm job (one chain at a time)]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__worktree__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-worktree-help-command-$line[1]:"
        case $line[1] in
            (ls)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prune)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(audit)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
'--nodes[Sweep every node from sinfo over ssh]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(session)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__session_commands" \
"*::: :->session" \
&& ret=0

    case $state in
    (session)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-session-command-$line[1]:"
        case $line[1] in
            (ensure)
_arguments "${_arguments_options[@]}" : \
'--node=[Request a specific compute node (\`--nodelist\`)]:NODE:_default' \
'-p+[Slurm partition]:PART:_default' \
'--partition=[Slurm partition]:PART:_default' \
'-t+[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'--time=[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'-j+[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'--threads=[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'-m+[Memory (\`--mem\`)]:SIZE:_default' \
'--mem=[Memory (\`--mem\`)]:SIZE:_default' \
'-n+[Tag the session with a name for easy reattach]:NAME:_default' \
'--name=[Tag the session with a name for easy reattach]:NAME:_default' \
'--mouse[Enable mouse support in the session]' \
'--no-mouse[Disable mouse support (overrides \`SINTERACTIVE_MOUSE\`)]' \
'--detach[Launch without attaching; print connection info and return]' \
'--json[Machine-readable JSON output (with \`--detach\`)]' \
'-h[Print help]' \
'--help[Print help]' \
':ensure_name -- Session name (the positional; distinct clap id from `--name`, which the flattened launch flags also carry):_default' \
&& ret=0
;;
(peek)
_arguments "${_arguments_options[@]}" : \
'-l+[How many lines from the bottom of the screen]:LINES:_default' \
'--lines=[How many lines from the bottom of the screen]:LINES:_default' \
'--pane=[Pane to read\: an id (\`terminal_3\` or \`3\`), a pane name, or \`focused\` (default \`terminal_0\`, the session'\''s shell)]:ID|NAME|focused:_default' \
'-h[Print help]' \
'--help[Print help]' \
':target -- JOBID, NAME, or list'\''s #; must be RUNNING:_default' \
&& ret=0
;;
(send)
_arguments "${_arguments_options[@]}" : \
'--pane=[Pane to type into\: an id (\`terminal_3\` or \`3\`), a pane name, or \`focused\` (default \`terminal_0\`, the session'\''s shell)]:ID|NAME|focused:_default' \
'-h[Print help]' \
'--help[Print help]' \
':target -- JOBID, NAME, or list'\''s #; must be RUNNING:_default' \
':command -- Typed into the session'\''s shell, followed by Enter:_default' \
&& ret=0
;;
(panes)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
':target:_default' \
&& ret=0
;;
(exec)
_arguments "${_arguments_options[@]}" : \
'--cwd=[Working directory for the command]:DIR:_default' \
'--timeout=[Give up waiting after this many seconds (the pane is then closed)]:SECS:_default' \
'--keep[Leave the pane open afterwards]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
':target:_default' \
':command -- Run with `bash -lc` in a new pane of the session:_default' \
&& ret=0
;;
(events)
_arguments "${_arguments_options[@]}" : \
'--since=[Only events after this epoch]:SINCE:_default' \
'--follow[Keep streaming as events are appended]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID or NAME; defaults to the current session:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__session__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-session-help-command-$line[1]:"
        case $line[1] in
            (ensure)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(peek)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(send)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(panes)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(exec)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(events)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(claude)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__claude_commands" \
"*::: :->claude" \
&& ret=0

    case $state in
    (claude)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-claude-command-$line[1]:"
        case $line[1] in
            (install)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(context)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(hook)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__claude__subcmd__hook_commands" \
"*::: :->hook" \
&& ret=0

    case $state in
    (hook)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-claude-hook-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__claude__subcmd__hook__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-claude-hook-help-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(statusline)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(mcp)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__claude__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-claude-help-command-$line[1]:"
        case $line[1] in
            (install)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(context)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(hook)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__claude__subcmd__help__subcmd__hook_commands" \
"*::: :->hook" \
&& ret=0

    case $state in
    (hook)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-claude-help-hook-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(statusline)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(mcp)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(gen)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__gen_commands" \
"*::: :->gen" \
&& ret=0

    case $state in
    (gen)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-gen-command-$line[1]:"
        case $line[1] in
            (completions)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':shell:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(man)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(schema)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__gen__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-gen-help-command-$line[1]:"
        case $line[1] in
            (completions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(man)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(schema)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(ensure)
_arguments "${_arguments_options[@]}" : \
'--node=[Request a specific compute node (\`--nodelist\`)]:NODE:_default' \
'-p+[Slurm partition]:PART:_default' \
'--partition=[Slurm partition]:PART:_default' \
'-t+[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'--time=[Wall time (\`8h\`, \`30m\`, \`1d12h\`, or Slurm \`D-HH\:MM\:SS\`)]:TIME:_default' \
'-j+[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'--threads=[Number of CPUs (\`--cpus-per-task\`)]:N:_default' \
'-m+[Memory (\`--mem\`)]:SIZE:_default' \
'--mem=[Memory (\`--mem\`)]:SIZE:_default' \
'-n+[Tag the session with a name for easy reattach]:NAME:_default' \
'--name=[Tag the session with a name for easy reattach]:NAME:_default' \
'--mouse[Enable mouse support in the session]' \
'--no-mouse[Disable mouse support (overrides \`SINTERACTIVE_MOUSE\`)]' \
'--detach[Launch without attaching; print connection info and return]' \
'--json[Machine-readable JSON output (with \`--detach\`)]' \
'-h[Print help]' \
'--help[Print help]' \
':ensure_name -- Session name (the positional; distinct clap id from `--name`, which the flattened launch flags also carry):_default' \
&& ret=0
;;
(peek)
_arguments "${_arguments_options[@]}" : \
'-l+[How many lines from the bottom of the screen]:LINES:_default' \
'--lines=[How many lines from the bottom of the screen]:LINES:_default' \
'--pane=[Pane to read\: an id (\`terminal_3\` or \`3\`), a pane name, or \`focused\` (default \`terminal_0\`, the session'\''s shell)]:ID|NAME|focused:_default' \
'-h[Print help]' \
'--help[Print help]' \
':target -- JOBID, NAME, or list'\''s #; must be RUNNING:_default' \
&& ret=0
;;
(send)
_arguments "${_arguments_options[@]}" : \
'--pane=[Pane to type into\: an id (\`terminal_3\` or \`3\`), a pane name, or \`focused\` (default \`terminal_0\`, the session'\''s shell)]:ID|NAME|focused:_default' \
'-h[Print help]' \
'--help[Print help]' \
':target -- JOBID, NAME, or list'\''s #; must be RUNNING:_default' \
':command -- Typed into the session'\''s shell, followed by Enter:_default' \
&& ret=0
;;
(events)
_arguments "${_arguments_options[@]}" : \
'--since=[Only events after this epoch]:SINCE:_default' \
'--follow[Keep streaming as events are appended]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID or NAME; defaults to the current session:_default' \
&& ret=0
;;
(refresh)
_arguments "${_arguments_options[@]}" : \
'--refresh[Re-check the time budget against Slurm now and update the cache]' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID, NAME, or list'\''s #; defaults to the current session:_default' \
&& ret=0
;;
(snapshot)
_arguments "${_arguments_options[@]}" : \
'--job=[Scope to this job'\''s cgroup on this host instead of the job this process runs in (what \`__job\` asks other nodes over ssh)]:JOBID:_default' \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(agent-context)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(hook)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_sinteractive__subcmd__hook_commands" \
"*::: :->hook" \
&& ret=0

    case $state in
    (hook)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-hook-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__hook__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-hook-help-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(statusline)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(mcp)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(install-claude)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':shell:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(man)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(schema)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(__job)
_arguments "${_arguments_options[@]}" : \
'--session-name=[]:SESSION_NAME:_default' \
'--maint=[\`NAME@EPOCH\` — maintenance reservation the request was trimmed to fit]:MAINT:_default' \
'--mouse[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(__attach)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':session:_default' \
&& ret=0
;;
(__popup)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':view:(monitor queue help notices rename)' \
'::job_id -- Defaults to `SINTERACTIVE_JOB_ID` (set in every session pane):_default' \
&& ret=0
;;
(__pane-cwd)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':job_id:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-command-$line[1]:"
        case $line[1] in
            (launch)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(attach)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(cancel)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(queue)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(monitor)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(quota)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__quota_commands" \
"*::: :->quota" \
&& ret=0

    case $state in
    (quota)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-quota-command-$line[1]:"
        case $line[1] in
            (check)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(top)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reindex)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(scratch)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__scratch_commands" \
"*::: :->scratch" \
&& ret=0

    case $state in
    (scratch)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-scratch-command-$line[1]:"
        case $line[1] in
            (new)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(ls)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(clean)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(worktree)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__worktree_commands" \
"*::: :->worktree" \
&& ret=0

    case $state in
    (worktree)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-worktree-command-$line[1]:"
        case $line[1] in
            (ls)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prune)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(audit)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(doctor)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(session)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__session_commands" \
"*::: :->session" \
&& ret=0

    case $state in
    (session)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-session-command-$line[1]:"
        case $line[1] in
            (ensure)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(peek)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(send)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(panes)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(exec)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(events)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(claude)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__claude_commands" \
"*::: :->claude" \
&& ret=0

    case $state in
    (claude)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-claude-command-$line[1]:"
        case $line[1] in
            (install)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(context)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(hook)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__claude__subcmd__hook_commands" \
"*::: :->hook" \
&& ret=0

    case $state in
    (hook)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-claude-hook-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(statusline)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(mcp)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(gen)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__gen_commands" \
"*::: :->gen" \
&& ret=0

    case $state in
    (gen)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-gen-command-$line[1]:"
        case $line[1] in
            (completions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(man)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(schema)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(ensure)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(peek)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(send)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(events)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(refresh)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(snapshot)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(agent-context)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(hook)
_arguments "${_arguments_options[@]}" : \
":: :_sinteractive__subcmd__help__subcmd__hook_commands" \
"*::: :->hook" \
&& ret=0

    case $state in
    (hook)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sinteractive-help-hook-command-$line[1]:"
        case $line[1] in
            (session-start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prompt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(worktree-remove)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(agent-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(scratch-guard)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(statusline)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(mcp)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(install-claude)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(man)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(schema)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(__job)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(__attach)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(__popup)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(__pane-cwd)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_sinteractive_commands] )) ||
_sinteractive_commands() {
    local commands; commands=(
'launch:Launch a new session (the default when no subcommand is given)' \
'attach:Reattach to a session by JOBID or NAME (your only session when omitted)' \
'list:List running sessions' \
'status:Show one session'\''s status' \
'cancel:Cancel a session' \
'queue:Your job queue\: running, pending (with reasons), and recent history' \
'monitor:Live CPU/GPU/process view of a session'\''s node, or any host' \
'quota:Storage quota (Bodhi quota daemons; unavailable on other clusters)' \
'scratch:Scratch directories you can list and clean by age\: new, ls, clean' \
'worktree:Worktrees whose work has landed\: ls, prune, audit' \
'doctor:Check this install and, optionally, every compute node' \
'session:Drive a session from outside\: ensure, peek, send, events' \
'claude:Claude Code integration\: install, context, hook, statusline, mcp' \
'gen:Generated output\: completions, man page, JSON schemas' \
'ensure:Superseded by \`session ensure\`' \
'peek:Superseded by \`session peek\`' \
'send:Superseded by \`session send\`' \
'events:Superseded by \`session events\`' \
'refresh:Superseded by \`status --refresh\`' \
'snapshot:Superseded by \`monitor --once\`' \
'agent-context:Superseded by \`claude context\`' \
'hook:Superseded by \`claude hook\`' \
'statusline:Superseded by \`claude statusline\`' \
'mcp:Superseded by \`claude mcp\`' \
'install-claude:Superseded by \`claude install\`' \
'completions:Superseded by \`gen completions\`' \
'man:Superseded by \`gen man\`' \
'schema:Superseded by \`gen schema\`' \
'__job:The batch job body\: starts zellij on the node and babysits it' \
'__attach:Runs on the node over ssh\: attach the local zellij client' \
'__popup:In-session floating views' \
'__pane-cwd:Runs on the node over ssh\: the pane'\''s live working directory' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd____attach_commands] )) ||
_sinteractive__subcmd____attach_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive __attach commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd____job_commands] )) ||
_sinteractive__subcmd____job_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive __job commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd____pane-cwd_commands] )) ||
_sinteractive__subcmd____pane-cwd_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive __pane-cwd commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd____popup_commands] )) ||
_sinteractive__subcmd____popup_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive __popup commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__agent-context_commands] )) ||
_sinteractive__subcmd__agent-context_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive agent-context commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__attach_commands] )) ||
_sinteractive__subcmd__attach_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive attach commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__cancel_commands] )) ||
_sinteractive__subcmd__cancel_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive cancel commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude_commands] )) ||
_sinteractive__subcmd__claude_commands() {
    local commands; commands=(
'install:Install the skills, hooks, statusline and MCP server' \
'context:Brief a coding agent on the session it is running inside' \
'hook:Hook entry points (Claude Code runs these)' \
'statusline:statusLine command (Claude Code runs this)' \
'mcp:MCP server over stdio (Claude Code runs this)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive claude commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__context_commands] )) ||
_sinteractive__subcmd__claude__subcmd__context_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude context commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help_commands() {
    local commands; commands=(
'install:Install the skills, hooks, statusline and MCP server' \
'context:Brief a coding agent on the session it is running inside' \
'hook:Hook entry points (Claude Code runs these)' \
'statusline:statusLine command (Claude Code runs this)' \
'mcp:MCP server over stdio (Claude Code runs this)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive claude help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__context_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__context_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help context commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
    )
    _describe -t commands 'sinteractive claude help hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help hook agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help hook scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help hook session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help hook worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__hook__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help hook worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__install_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help install commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__mcp_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__mcp_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help mcp commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__help__subcmd__statusline_commands] )) ||
_sinteractive__subcmd__claude__subcmd__help__subcmd__statusline_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude help statusline commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive claude hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive claude hook help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__help__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook help worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__hook__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__claude__subcmd__hook__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude hook worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__install_commands] )) ||
_sinteractive__subcmd__claude__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude install commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__mcp_commands] )) ||
_sinteractive__subcmd__claude__subcmd__mcp_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude mcp commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__claude__subcmd__statusline_commands] )) ||
_sinteractive__subcmd__claude__subcmd__statusline_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive claude statusline commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__completions_commands] )) ||
_sinteractive__subcmd__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive completions commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__doctor_commands] )) ||
_sinteractive__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive doctor commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__ensure_commands] )) ||
_sinteractive__subcmd__ensure_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive ensure commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__events_commands] )) ||
_sinteractive__subcmd__events_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive events commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen_commands] )) ||
_sinteractive__subcmd__gen_commands() {
    local commands; commands=(
'completions:Shell completions' \
'man:The man page (roff)' \
'schema:The JSON schemas of the machine-readable outputs' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive gen commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__completions_commands] )) ||
_sinteractive__subcmd__gen__subcmd__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen completions commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__help_commands] )) ||
_sinteractive__subcmd__gen__subcmd__help_commands() {
    local commands; commands=(
'completions:Shell completions' \
'man:The man page (roff)' \
'schema:The JSON schemas of the machine-readable outputs' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive gen help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__help__subcmd__completions_commands] )) ||
_sinteractive__subcmd__gen__subcmd__help__subcmd__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen help completions commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__gen__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__help__subcmd__man_commands] )) ||
_sinteractive__subcmd__gen__subcmd__help__subcmd__man_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen help man commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__help__subcmd__schema_commands] )) ||
_sinteractive__subcmd__gen__subcmd__help__subcmd__schema_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen help schema commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__man_commands] )) ||
_sinteractive__subcmd__gen__subcmd__man_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen man commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__gen__subcmd__schema_commands] )) ||
_sinteractive__subcmd__gen__subcmd__schema_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive gen schema commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help_commands] )) ||
_sinteractive__subcmd__help_commands() {
    local commands; commands=(
'launch:Launch a new session (the default when no subcommand is given)' \
'attach:Reattach to a session by JOBID or NAME (your only session when omitted)' \
'list:List running sessions' \
'status:Show one session'\''s status' \
'cancel:Cancel a session' \
'queue:Your job queue\: running, pending (with reasons), and recent history' \
'monitor:Live CPU/GPU/process view of a session'\''s node, or any host' \
'quota:Storage quota (Bodhi quota daemons; unavailable on other clusters)' \
'scratch:Scratch directories you can list and clean by age\: new, ls, clean' \
'worktree:Worktrees whose work has landed\: ls, prune, audit' \
'doctor:Check this install and, optionally, every compute node' \
'session:Drive a session from outside\: ensure, peek, send, events' \
'claude:Claude Code integration\: install, context, hook, statusline, mcp' \
'gen:Generated output\: completions, man page, JSON schemas' \
'ensure:Superseded by \`session ensure\`' \
'peek:Superseded by \`session peek\`' \
'send:Superseded by \`session send\`' \
'events:Superseded by \`session events\`' \
'refresh:Superseded by \`status --refresh\`' \
'snapshot:Superseded by \`monitor --once\`' \
'agent-context:Superseded by \`claude context\`' \
'hook:Superseded by \`claude hook\`' \
'statusline:Superseded by \`claude statusline\`' \
'mcp:Superseded by \`claude mcp\`' \
'install-claude:Superseded by \`claude install\`' \
'completions:Superseded by \`gen completions\`' \
'man:Superseded by \`gen man\`' \
'schema:Superseded by \`gen schema\`' \
'__job:The batch job body\: starts zellij on the node and babysits it' \
'__attach:Runs on the node over ssh\: attach the local zellij client' \
'__popup:In-session floating views' \
'__pane-cwd:Runs on the node over ssh\: the pane'\''s live working directory' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd____attach_commands] )) ||
_sinteractive__subcmd__help__subcmd____attach_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help __attach commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd____job_commands] )) ||
_sinteractive__subcmd__help__subcmd____job_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help __job commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd____pane-cwd_commands] )) ||
_sinteractive__subcmd__help__subcmd____pane-cwd_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help __pane-cwd commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd____popup_commands] )) ||
_sinteractive__subcmd__help__subcmd____popup_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help __popup commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__agent-context_commands] )) ||
_sinteractive__subcmd__help__subcmd__agent-context_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help agent-context commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__attach_commands] )) ||
_sinteractive__subcmd__help__subcmd__attach_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help attach commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__cancel_commands] )) ||
_sinteractive__subcmd__help__subcmd__cancel_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help cancel commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude_commands() {
    local commands; commands=(
'install:Install the skills, hooks, statusline and MCP server' \
'context:Brief a coding agent on the session it is running inside' \
'hook:Hook entry points (Claude Code runs these)' \
'statusline:statusLine command (Claude Code runs this)' \
'mcp:MCP server over stdio (Claude Code runs this)' \
    )
    _describe -t commands 'sinteractive help claude commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__context_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__context_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude context commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
    )
    _describe -t commands 'sinteractive help claude hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude hook agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude hook scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude hook session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude hook worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__hook__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude hook worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__install_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__install_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude install commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__mcp_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__mcp_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude mcp commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__claude__subcmd__statusline_commands] )) ||
_sinteractive__subcmd__help__subcmd__claude__subcmd__statusline_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help claude statusline commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__completions_commands] )) ||
_sinteractive__subcmd__help__subcmd__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help completions commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__doctor_commands] )) ||
_sinteractive__subcmd__help__subcmd__doctor_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help doctor commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__ensure_commands] )) ||
_sinteractive__subcmd__help__subcmd__ensure_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help ensure commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__events_commands] )) ||
_sinteractive__subcmd__help__subcmd__events_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help events commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__gen_commands] )) ||
_sinteractive__subcmd__help__subcmd__gen_commands() {
    local commands; commands=(
'completions:Shell completions' \
'man:The man page (roff)' \
'schema:The JSON schemas of the machine-readable outputs' \
    )
    _describe -t commands 'sinteractive help gen commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__gen__subcmd__completions_commands] )) ||
_sinteractive__subcmd__help__subcmd__gen__subcmd__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help gen completions commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__gen__subcmd__man_commands] )) ||
_sinteractive__subcmd__help__subcmd__gen__subcmd__man_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help gen man commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__gen__subcmd__schema_commands] )) ||
_sinteractive__subcmd__help__subcmd__gen__subcmd__schema_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help gen schema commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
    )
    _describe -t commands 'sinteractive help hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__install-claude_commands] )) ||
_sinteractive__subcmd__help__subcmd__install-claude_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help install-claude commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__launch_commands] )) ||
_sinteractive__subcmd__help__subcmd__launch_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help launch commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__list_commands] )) ||
_sinteractive__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help list commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__man_commands] )) ||
_sinteractive__subcmd__help__subcmd__man_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help man commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__mcp_commands] )) ||
_sinteractive__subcmd__help__subcmd__mcp_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help mcp commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__monitor_commands] )) ||
_sinteractive__subcmd__help__subcmd__monitor_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help monitor commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__peek_commands] )) ||
_sinteractive__subcmd__help__subcmd__peek_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help peek commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__queue_commands] )) ||
_sinteractive__subcmd__help__subcmd__queue_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help queue commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__quota_commands] )) ||
_sinteractive__subcmd__help__subcmd__quota_commands() {
    local commands; commands=(
'check:Probe now instead of reading the cache, and poke every running session' \
'top:What is using the quota\: the largest directories and probable duplicate files' \
'reindex:Rebuild the usage index, as a batch job (a walk of home takes a while)' \
    )
    _describe -t commands 'sinteractive help quota commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__quota__subcmd__check_commands] )) ||
_sinteractive__subcmd__help__subcmd__quota__subcmd__check_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help quota check commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__quota__subcmd__reindex_commands] )) ||
_sinteractive__subcmd__help__subcmd__quota__subcmd__reindex_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help quota reindex commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__quota__subcmd__top_commands] )) ||
_sinteractive__subcmd__help__subcmd__quota__subcmd__top_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help quota top commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__refresh_commands] )) ||
_sinteractive__subcmd__help__subcmd__refresh_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help refresh commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__schema_commands] )) ||
_sinteractive__subcmd__help__subcmd__schema_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help schema commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__scratch_commands] )) ||
_sinteractive__subcmd__help__subcmd__scratch_commands() {
    local commands; commands=(
'new:Make (or find) a scratch directory for TOPIC and print its path' \
'ls:List scratch directories\: size, age, branch (and whether it still exists)' \
'clean:Remove scratch directories older than AGE; lists them unless --yes' \
    )
    _describe -t commands 'sinteractive help scratch commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__scratch__subcmd__clean_commands] )) ||
_sinteractive__subcmd__help__subcmd__scratch__subcmd__clean_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help scratch clean commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__scratch__subcmd__ls_commands] )) ||
_sinteractive__subcmd__help__subcmd__scratch__subcmd__ls_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help scratch ls commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__scratch__subcmd__new_commands] )) ||
_sinteractive__subcmd__help__subcmd__scratch__subcmd__new_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help scratch new commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__send_commands] )) ||
_sinteractive__subcmd__help__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help send commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session_commands] )) ||
_sinteractive__subcmd__help__subcmd__session_commands() {
    local commands; commands=(
'ensure:Reuse the session named NAME, or launch it if absent (implies --detach)' \
'peek:Read the last lines of a session'\''s screen' \
'send:Type a command into a session'\''s shell' \
'panes:List a session'\''s panes' \
'exec:Run a short command in a new pane and wait for its output' \
'events:Stream session events (NDJSON)' \
    )
    _describe -t commands 'sinteractive help session commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session__subcmd__ensure_commands] )) ||
_sinteractive__subcmd__help__subcmd__session__subcmd__ensure_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help session ensure commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session__subcmd__events_commands] )) ||
_sinteractive__subcmd__help__subcmd__session__subcmd__events_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help session events commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session__subcmd__exec_commands] )) ||
_sinteractive__subcmd__help__subcmd__session__subcmd__exec_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help session exec commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session__subcmd__panes_commands] )) ||
_sinteractive__subcmd__help__subcmd__session__subcmd__panes_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help session panes commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session__subcmd__peek_commands] )) ||
_sinteractive__subcmd__help__subcmd__session__subcmd__peek_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help session peek commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__session__subcmd__send_commands] )) ||
_sinteractive__subcmd__help__subcmd__session__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help session send commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__snapshot_commands] )) ||
_sinteractive__subcmd__help__subcmd__snapshot_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help snapshot commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__status_commands] )) ||
_sinteractive__subcmd__help__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help status commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__statusline_commands] )) ||
_sinteractive__subcmd__help__subcmd__statusline_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help statusline commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__worktree_commands] )) ||
_sinteractive__subcmd__help__subcmd__worktree_commands() {
    local commands; commands=(
'ls:List worktrees\: size, age, state (merged, open, local, ...), and which are removable' \
'prune:Remove worktrees whose work has landed; lists them unless --yes' \
'audit:Write a report of removable worktrees to scratch; never deletes' \
    )
    _describe -t commands 'sinteractive help worktree commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__worktree__subcmd__audit_commands] )) ||
_sinteractive__subcmd__help__subcmd__worktree__subcmd__audit_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help worktree audit commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__worktree__subcmd__ls_commands] )) ||
_sinteractive__subcmd__help__subcmd__worktree__subcmd__ls_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help worktree ls commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__worktree__subcmd__prune_commands] )) ||
_sinteractive__subcmd__help__subcmd__worktree__subcmd__prune_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help worktree prune commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook_commands] )) ||
_sinteractive__subcmd__hook_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__hook__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'worktree-create:WorktreeCreate\: make the worktree on the cluster'\''s scratch filesystem' \
'worktree-remove:WorktreeRemove\: remove a worktree the create hook made' \
'agent-guard:PreToolUse (Agent)\: deny a subagent launched without a model; confirm Fable ones' \
'scratch-guard:PreToolUse (Bash, Write, Edit)\: deny bulk data copied into a worktree and new directories under ~/.cache' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive hook help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__agent-guard_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__agent-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help agent-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__scratch-guard_commands] )) ||
_sinteractive__subcmd__hook__subcmd__scratch-guard_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook scratch-guard commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__worktree-create_commands] )) ||
_sinteractive__subcmd__hook__subcmd__worktree-create_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook worktree-create commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__worktree-remove_commands] )) ||
_sinteractive__subcmd__hook__subcmd__worktree-remove_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook worktree-remove commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__install-claude_commands] )) ||
_sinteractive__subcmd__install-claude_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive install-claude commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__launch_commands] )) ||
_sinteractive__subcmd__launch_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive launch commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__list_commands] )) ||
_sinteractive__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive list commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__man_commands] )) ||
_sinteractive__subcmd__man_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive man commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__mcp_commands] )) ||
_sinteractive__subcmd__mcp_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive mcp commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__monitor_commands] )) ||
_sinteractive__subcmd__monitor_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive monitor commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__peek_commands] )) ||
_sinteractive__subcmd__peek_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive peek commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__queue_commands] )) ||
_sinteractive__subcmd__queue_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive queue commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota_commands] )) ||
_sinteractive__subcmd__quota_commands() {
    local commands; commands=(
'check:Probe now instead of reading the cache, and poke every running session' \
'top:What is using the quota\: the largest directories and probable duplicate files' \
'reindex:Rebuild the usage index, as a batch job (a walk of home takes a while)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive quota commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__check_commands] )) ||
_sinteractive__subcmd__quota__subcmd__check_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota check commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__help_commands] )) ||
_sinteractive__subcmd__quota__subcmd__help_commands() {
    local commands; commands=(
'check:Probe now instead of reading the cache, and poke every running session' \
'top:What is using the quota\: the largest directories and probable duplicate files' \
'reindex:Rebuild the usage index, as a batch job (a walk of home takes a while)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive quota help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__help__subcmd__check_commands] )) ||
_sinteractive__subcmd__quota__subcmd__help__subcmd__check_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota help check commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__quota__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__help__subcmd__reindex_commands] )) ||
_sinteractive__subcmd__quota__subcmd__help__subcmd__reindex_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota help reindex commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__help__subcmd__top_commands] )) ||
_sinteractive__subcmd__quota__subcmd__help__subcmd__top_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota help top commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__reindex_commands] )) ||
_sinteractive__subcmd__quota__subcmd__reindex_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota reindex commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__quota__subcmd__top_commands] )) ||
_sinteractive__subcmd__quota__subcmd__top_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive quota top commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__refresh_commands] )) ||
_sinteractive__subcmd__refresh_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive refresh commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__schema_commands] )) ||
_sinteractive__subcmd__schema_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive schema commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch_commands] )) ||
_sinteractive__subcmd__scratch_commands() {
    local commands; commands=(
'new:Make (or find) a scratch directory for TOPIC and print its path' \
'ls:List scratch directories\: size, age, branch (and whether it still exists)' \
'clean:Remove scratch directories older than AGE; lists them unless --yes' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive scratch commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__clean_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__clean_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch clean commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__help_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__help_commands() {
    local commands; commands=(
'new:Make (or find) a scratch directory for TOPIC and print its path' \
'ls:List scratch directories\: size, age, branch (and whether it still exists)' \
'clean:Remove scratch directories older than AGE; lists them unless --yes' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive scratch help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__help__subcmd__clean_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__help__subcmd__clean_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch help clean commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__help__subcmd__ls_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__help__subcmd__ls_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch help ls commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__help__subcmd__new_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__help__subcmd__new_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch help new commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__ls_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__ls_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch ls commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__scratch__subcmd__new_commands] )) ||
_sinteractive__subcmd__scratch__subcmd__new_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive scratch new commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__send_commands] )) ||
_sinteractive__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive send commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session_commands] )) ||
_sinteractive__subcmd__session_commands() {
    local commands; commands=(
'ensure:Reuse the session named NAME, or launch it if absent (implies --detach)' \
'peek:Read the last lines of a session'\''s screen' \
'send:Type a command into a session'\''s shell' \
'panes:List a session'\''s panes' \
'exec:Run a short command in a new pane and wait for its output' \
'events:Stream session events (NDJSON)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive session commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__ensure_commands] )) ||
_sinteractive__subcmd__session__subcmd__ensure_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session ensure commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__events_commands] )) ||
_sinteractive__subcmd__session__subcmd__events_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session events commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__exec_commands] )) ||
_sinteractive__subcmd__session__subcmd__exec_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session exec commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help_commands] )) ||
_sinteractive__subcmd__session__subcmd__help_commands() {
    local commands; commands=(
'ensure:Reuse the session named NAME, or launch it if absent (implies --detach)' \
'peek:Read the last lines of a session'\''s screen' \
'send:Type a command into a session'\''s shell' \
'panes:List a session'\''s panes' \
'exec:Run a short command in a new pane and wait for its output' \
'events:Stream session events (NDJSON)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive session help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__ensure_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__ensure_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help ensure commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__events_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__events_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help events commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__exec_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__exec_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help exec commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__panes_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__panes_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help panes commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__peek_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__peek_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help peek commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__help__subcmd__send_commands] )) ||
_sinteractive__subcmd__session__subcmd__help__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session help send commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__panes_commands] )) ||
_sinteractive__subcmd__session__subcmd__panes_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session panes commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__peek_commands] )) ||
_sinteractive__subcmd__session__subcmd__peek_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session peek commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__session__subcmd__send_commands] )) ||
_sinteractive__subcmd__session__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive session send commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__snapshot_commands] )) ||
_sinteractive__subcmd__snapshot_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive snapshot commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__status_commands] )) ||
_sinteractive__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive status commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__statusline_commands] )) ||
_sinteractive__subcmd__statusline_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive statusline commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree_commands] )) ||
_sinteractive__subcmd__worktree_commands() {
    local commands; commands=(
'ls:List worktrees\: size, age, state (merged, open, local, ...), and which are removable' \
'prune:Remove worktrees whose work has landed; lists them unless --yes' \
'audit:Write a report of removable worktrees to scratch; never deletes' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive worktree commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__audit_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__audit_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree audit commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__help_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__help_commands() {
    local commands; commands=(
'ls:List worktrees\: size, age, state (merged, open, local, ...), and which are removable' \
'prune:Remove worktrees whose work has landed; lists them unless --yes' \
'audit:Write a report of removable worktrees to scratch; never deletes' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive worktree help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__help__subcmd__audit_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__help__subcmd__audit_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree help audit commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__help__subcmd__help_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree help help commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__help__subcmd__ls_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__help__subcmd__ls_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree help ls commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__help__subcmd__prune_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__help__subcmd__prune_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree help prune commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__ls_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__ls_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree ls commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__worktree__subcmd__prune_commands] )) ||
_sinteractive__subcmd__worktree__subcmd__prune_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive worktree prune commands' commands "$@"
}

if [ "$funcstack[1]" = "_sinteractive" ]; then
    _sinteractive "$@"
else
    compdef _sinteractive sinteractive
fi
