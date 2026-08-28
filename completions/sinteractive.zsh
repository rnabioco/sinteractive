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
'::target -- JOBID or NAME; with no target, your only session:_default' \
&& ret=0
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
(status)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID or NAME; defaults to the current session:_default' \
&& ret=0
;;
(refresh)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID or NAME; defaults to the current session:_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(cancel)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
':target:_default' \
&& ret=0
;;
(queue)
_arguments "${_arguments_options[@]}" : \
'--all[Include every user'\''s jobs in the partitions you can see]' \
'--watch[Refresh continuously]' \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(monitor)
_arguments "${_arguments_options[@]}" : \
'--live[Sample over ssh at 1 Hz instead of reading the shared snapshot]' \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
'::target -- JOBID, NAME, or a hostname; defaults to the current session:_default' \
&& ret=0
;;
(snapshot)
_arguments "${_arguments_options[@]}" : \
'--json[Machine-readable JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(events)
_arguments "${_arguments_options[@]}" : \
'--since=[Only events after this epoch]:SINCE:_default' \
'--follow[]' \
'-h[Print help]' \
'--help[Print help]' \
'::target:_default' \
&& ret=0
;;
(peek)
_arguments "${_arguments_options[@]}" : \
'-n+[]:LINES:_default' \
'--lines=[]:LINES:_default' \
'-h[Print help]' \
'--help[Print help]' \
':target:_default' \
&& ret=0
;;
(send)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':target:_default' \
':command:_default' \
&& ret=0
;;
(agent-context)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(quota)
_arguments "${_arguments_options[@]}" : \
'--check[Probe now instead of reading the cache, and poke every running session]' \
'--json[]' \
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
(doctor)
_arguments "${_arguments_options[@]}" : \
'--nodes[Sweep every node from sinfo over ssh]' \
'--json[]' \
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
':view:(monitor queue help notices)' \
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
(ensure)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(refresh)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
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
(snapshot)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(events)
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
(agent-context)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(quota)
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
(doctor)
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
'ensure:Reuse the session named NAME, or launch it if absent (implies --detach)' \
'status:Show one session'\''s status' \
'refresh:Re-check a session'\''s time budget now and update its cache' \
'list:List running sessions' \
'cancel:Cancel a session' \
'queue:Your job queue\: running, pending (with reasons), and recent history' \
'monitor:Live CPU/GPU/process view of a session'\''s node, or any host' \
'snapshot:One-shot resource sample of this host' \
'events:Stream session events (NDJSON)' \
'peek:Read the last lines of a session'\''s screen' \
'send:Type a command into a session'\''s shell' \
'agent-context:Brief a coding agent on the session it is running inside' \
'quota:Storage quota (Bodhi daemons; wraps curc-quota on Alpine)' \
'hook:Claude Code hook entry points' \
'statusline:Claude Code statusLine command' \
'mcp:MCP server over stdio' \
'install-claude:Install the Claude Code skills, hooks, statusline and MCP server' \
'doctor:Check this install and, optionally, every compute node' \
'completions:Print shell completions' \
'man:Print the man page (roff)' \
'schema:Dump the JSON schemas of the machine-readable outputs' \
'__job:The batch job body\: starts zellij on the node and babysits it' \
'__attach:Runs on the node over ssh\: attach the local zellij client' \
'__popup:In-session floating views' \
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
(( $+functions[_sinteractive__subcmd__help_commands] )) ||
_sinteractive__subcmd__help_commands() {
    local commands; commands=(
'launch:Launch a new session (the default when no subcommand is given)' \
'attach:Reattach to a session by JOBID or NAME (your only session when omitted)' \
'ensure:Reuse the session named NAME, or launch it if absent (implies --detach)' \
'status:Show one session'\''s status' \
'refresh:Re-check a session'\''s time budget now and update its cache' \
'list:List running sessions' \
'cancel:Cancel a session' \
'queue:Your job queue\: running, pending (with reasons), and recent history' \
'monitor:Live CPU/GPU/process view of a session'\''s node, or any host' \
'snapshot:One-shot resource sample of this host' \
'events:Stream session events (NDJSON)' \
'peek:Read the last lines of a session'\''s screen' \
'send:Type a command into a session'\''s shell' \
'agent-context:Brief a coding agent on the session it is running inside' \
'quota:Storage quota (Bodhi daemons; wraps curc-quota on Alpine)' \
'hook:Claude Code hook entry points' \
'statusline:Claude Code statusLine command' \
'mcp:MCP server over stdio' \
'install-claude:Install the Claude Code skills, hooks, statusline and MCP server' \
'doctor:Check this install and, optionally, every compute node' \
'completions:Print shell completions' \
'man:Print the man page (roff)' \
'schema:Dump the JSON schemas of the machine-readable outputs' \
'__job:The batch job body\: starts zellij on the node and babysits it' \
'__attach:Runs on the node over ssh\: attach the local zellij client' \
'__popup:In-session floating views' \
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
    )
    _describe -t commands 'sinteractive help hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__help__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__help__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help hook session-start commands' commands "$@"
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
    local commands; commands=()
    _describe -t commands 'sinteractive help quota commands' commands "$@"
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
(( $+functions[_sinteractive__subcmd__help__subcmd__send_commands] )) ||
_sinteractive__subcmd__help__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive help send commands' commands "$@"
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
(( $+functions[_sinteractive__subcmd__hook_commands] )) ||
_sinteractive__subcmd__hook_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive hook commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__help_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help_commands() {
    local commands; commands=(
'session-start:SessionStart\: print the agent briefing' \
'prompt:UserPromptSubmit\: warn when walltime is short' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sinteractive hook help commands' commands "$@"
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
(( $+functions[_sinteractive__subcmd__hook__subcmd__help__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__hook__subcmd__help__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook help session-start commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__prompt_commands] )) ||
_sinteractive__subcmd__hook__subcmd__prompt_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook prompt commands' commands "$@"
}
(( $+functions[_sinteractive__subcmd__hook__subcmd__session-start_commands] )) ||
_sinteractive__subcmd__hook__subcmd__session-start_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive hook session-start commands' commands "$@"
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
    local commands; commands=()
    _describe -t commands 'sinteractive quota commands' commands "$@"
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
(( $+functions[_sinteractive__subcmd__send_commands] )) ||
_sinteractive__subcmd__send_commands() {
    local commands; commands=()
    _describe -t commands 'sinteractive send commands' commands "$@"
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

if [ "$funcstack[1]" = "_sinteractive" ]; then
    _sinteractive "$@"
else
    compdef _sinteractive sinteractive
fi
