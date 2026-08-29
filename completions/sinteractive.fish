# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_sinteractive_global_optspecs
    string join \n node= p/partition= t/time= j/threads= m/mem= n/name= mouse no-mouse detach json status= refresh= l/list ensure= a/attach= cancel= check-quota agent-context install-claude h/help V/version
end

function __fish_sinteractive_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_sinteractive_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_sinteractive_using_subcommand
    set -l cmd (__fish_sinteractive_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c sinteractive -n "__fish_sinteractive_needs_command" -l node -d 'Request a specific compute node (`--nodelist`)' -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s p -l partition -d 'Slurm partition' -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s t -l time -d 'Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`)' -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s j -l threads -d 'Number of CPUs (`--cpus-per-task`)' -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s m -l mem -d 'Memory (`--mem`)' -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s n -l name -d 'Tag the session with a name for easy reattach' -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l status -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l refresh -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l ensure -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s a -l attach -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l cancel -r
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l mouse -d 'Enable mouse support in the session'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l no-mouse -d 'Disable mouse support (overrides `SINTERACTIVE_MOUSE`)'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l detach -d 'Launch without attaching; print connection info and return'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l json -d 'Machine-readable JSON output (with `--detach`)'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s l -l list
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l check-quota
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l agent-context
complete -c sinteractive -n "__fish_sinteractive_needs_command" -l install-claude
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -s V -l version -d 'Print version'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "launch" -d 'Launch a new session (the default when no subcommand is given)'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "attach" -d 'Reattach to a session by JOBID or NAME (your only session when omitted)'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "list" -d 'List running sessions'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "status" -d 'Show one session\'s status'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "cancel" -d 'Cancel a session'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "queue" -d 'Your job queue: running, pending (with reasons), and recent history'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "monitor" -d 'Live CPU/GPU/process view of a session\'s node, or any host'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "quota" -d 'Storage quota (Bodhi quota daemons; unavailable on other clusters)'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "doctor" -d 'Check this install and, optionally, every compute node'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "session" -d 'Drive a session from outside: ensure, peek, send, events'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "claude" -d 'Claude Code integration: install, context, hook, statusline, mcp'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "gen" -d 'Generated output: completions, man page, JSON schemas'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "ensure" -d 'Superseded by `session ensure`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "peek" -d 'Superseded by `session peek`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "send" -d 'Superseded by `session send`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "events" -d 'Superseded by `session events`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "refresh" -d 'Superseded by `status --refresh`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "snapshot" -d 'Superseded by `monitor --once`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "agent-context" -d 'Superseded by `claude context`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "hook" -d 'Superseded by `claude hook`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "statusline" -d 'Superseded by `claude statusline`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "mcp" -d 'Superseded by `claude mcp`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "install-claude" -d 'Superseded by `claude install`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "completions" -d 'Superseded by `gen completions`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "man" -d 'Superseded by `gen man`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "schema" -d 'Superseded by `gen schema`'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "__job" -d 'The batch job body: starts zellij on the node and babysits it'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "__attach" -d 'Runs on the node over ssh: attach the local zellij client'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "__popup" -d 'In-session floating views'
complete -c sinteractive -n "__fish_sinteractive_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -l node -d 'Request a specific compute node (`--nodelist`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -s p -l partition -d 'Slurm partition' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -s t -l time -d 'Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -s j -l threads -d 'Number of CPUs (`--cpus-per-task`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -s m -l mem -d 'Memory (`--mem`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -s n -l name -d 'Tag the session with a name for easy reattach' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -l mouse -d 'Enable mouse support in the session'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -l no-mouse -d 'Disable mouse support (overrides `SINTERACTIVE_MOUSE`)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -l detach -d 'Launch without attaching; print connection info and return'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -l json -d 'Machine-readable JSON output (with `--detach`)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand launch" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand attach" -l ssh -d 'Attach over ssh -X (X11 forwarding) instead of srun --overlap'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand attach" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand list" -l json -d 'Machine-readable JSON output'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand list" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand status" -l refresh -d 'Re-check the time budget against Slurm now and update the cache'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand status" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand status" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand cancel" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand cancel" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand queue" -l all -d 'Include every user\'s jobs in the partitions you can see'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand queue" -l watch -d 'Refresh continuously'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand queue" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand queue" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand monitor" -l job -d 'With --once and no TARGET: scope to this job\'s cgroup on this host' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand monitor" -l live -d 'Sample over ssh at 1 Hz instead of reading the shared snapshot'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand monitor" -l once -d 'Print one sample and exit instead of opening the live view'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand monitor" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand monitor" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand quota" -l check -d 'Probe now instead of reading the cache, and poke every running session'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand quota" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand quota" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand doctor" -l nodes -d 'Sweep every node from sinfo over ssh'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand doctor" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand doctor" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and not __fish_seen_subcommand_from ensure peek send events help" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and not __fish_seen_subcommand_from ensure peek send events help" -f -a "ensure" -d 'Reuse the session named NAME, or launch it if absent (implies --detach)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and not __fish_seen_subcommand_from ensure peek send events help" -f -a "peek" -d 'Read the last lines of a session\'s screen'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and not __fish_seen_subcommand_from ensure peek send events help" -f -a "send" -d 'Type a command into a session\'s shell'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and not __fish_seen_subcommand_from ensure peek send events help" -f -a "events" -d 'Stream session events (NDJSON)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and not __fish_seen_subcommand_from ensure peek send events help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -l node -d 'Request a specific compute node (`--nodelist`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -s p -l partition -d 'Slurm partition' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -s t -l time -d 'Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -s j -l threads -d 'Number of CPUs (`--cpus-per-task`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -s m -l mem -d 'Memory (`--mem`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -s n -l name -d 'Tag the session with a name for easy reattach' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -l mouse -d 'Enable mouse support in the session'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -l no-mouse -d 'Disable mouse support (overrides `SINTERACTIVE_MOUSE`)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -l detach -d 'Launch without attaching; print connection info and return'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -l json -d 'Machine-readable JSON output (with `--detach`)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from ensure" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from peek" -s n -l lines -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from peek" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from send" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from events" -l since -d 'Only events after this epoch' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from events" -l follow
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from events" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "ensure" -d 'Reuse the session named NAME, or launch it if absent (implies --detach)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "peek" -d 'Read the last lines of a session\'s screen'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "send" -d 'Type a command into a session\'s shell'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "events" -d 'Stream session events (NDJSON)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -f -a "install" -d 'Install the skills, hooks, statusline and MCP server'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -f -a "context" -d 'Brief a coding agent on the session it is running inside'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -f -a "hook" -d 'Hook entry points (Claude Code runs these)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -f -a "statusline" -d 'statusLine command (Claude Code runs this)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -f -a "mcp" -d 'MCP server over stdio (Claude Code runs this)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and not __fish_seen_subcommand_from install context hook statusline mcp help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from install" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from context" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from hook" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from hook" -f -a "session-start" -d 'SessionStart: print the agent briefing'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from hook" -f -a "prompt" -d 'UserPromptSubmit: warn when walltime is short'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from hook" -f -a "worktree-create" -d 'WorktreeCreate: make the worktree on the cluster\'s scratch filesystem'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from hook" -f -a "worktree-remove" -d 'WorktreeRemove: remove a worktree the create hook made'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from hook" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from statusline" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from mcp" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from help" -f -a "install" -d 'Install the skills, hooks, statusline and MCP server'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from help" -f -a "context" -d 'Brief a coding agent on the session it is running inside'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from help" -f -a "hook" -d 'Hook entry points (Claude Code runs these)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from help" -f -a "statusline" -d 'statusLine command (Claude Code runs this)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from help" -f -a "mcp" -d 'MCP server over stdio (Claude Code runs this)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand claude; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and not __fish_seen_subcommand_from completions man schema help" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and not __fish_seen_subcommand_from completions man schema help" -f -a "completions" -d 'Shell completions'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and not __fish_seen_subcommand_from completions man schema help" -f -a "man" -d 'The man page (roff)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and not __fish_seen_subcommand_from completions man schema help" -f -a "schema" -d 'The JSON schemas of the machine-readable outputs'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and not __fish_seen_subcommand_from completions man schema help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from completions" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from man" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from schema" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from help" -f -a "completions" -d 'Shell completions'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from help" -f -a "man" -d 'The man page (roff)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from help" -f -a "schema" -d 'The JSON schemas of the machine-readable outputs'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand gen; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -l node -d 'Request a specific compute node (`--nodelist`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -s p -l partition -d 'Slurm partition' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -s t -l time -d 'Wall time (`8h`, `30m`, `1d12h`, or Slurm `D-HH:MM:SS`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -s j -l threads -d 'Number of CPUs (`--cpus-per-task`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -s m -l mem -d 'Memory (`--mem`)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -s n -l name -d 'Tag the session with a name for easy reattach' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -l mouse -d 'Enable mouse support in the session'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -l no-mouse -d 'Disable mouse support (overrides `SINTERACTIVE_MOUSE`)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -l detach -d 'Launch without attaching; print connection info and return'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -l json -d 'Machine-readable JSON output (with `--detach`)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand ensure" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand peek" -s n -l lines -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand peek" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand send" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand events" -l since -d 'Only events after this epoch' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand events" -l follow
complete -c sinteractive -n "__fish_sinteractive_using_subcommand events" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand refresh" -l refresh -d 'Re-check the time budget against Slurm now and update the cache'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand refresh" -l json
complete -c sinteractive -n "__fish_sinteractive_using_subcommand refresh" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand snapshot" -l job -d 'Scope to this job\'s cgroup on this host instead of the job this process runs in (what `__job` asks other nodes over ssh)' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand snapshot" -l json -d 'Machine-readable JSON output'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand snapshot" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand agent-context" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and not __fish_seen_subcommand_from session-start prompt worktree-create worktree-remove help" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and not __fish_seen_subcommand_from session-start prompt worktree-create worktree-remove help" -f -a "session-start" -d 'SessionStart: print the agent briefing'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and not __fish_seen_subcommand_from session-start prompt worktree-create worktree-remove help" -f -a "prompt" -d 'UserPromptSubmit: warn when walltime is short'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and not __fish_seen_subcommand_from session-start prompt worktree-create worktree-remove help" -f -a "worktree-create" -d 'WorktreeCreate: make the worktree on the cluster\'s scratch filesystem'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and not __fish_seen_subcommand_from session-start prompt worktree-create worktree-remove help" -f -a "worktree-remove" -d 'WorktreeRemove: remove a worktree the create hook made'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and not __fish_seen_subcommand_from session-start prompt worktree-create worktree-remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from session-start" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from prompt" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from worktree-create" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from worktree-remove" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "session-start" -d 'SessionStart: print the agent briefing'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "prompt" -d 'UserPromptSubmit: warn when walltime is short'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "worktree-create" -d 'WorktreeCreate: make the worktree on the cluster\'s scratch filesystem'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "worktree-remove" -d 'WorktreeRemove: remove a worktree the create hook made'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand statusline" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand mcp" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand install-claude" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand completions" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand man" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand schema" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand __job" -l session-name -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand __job" -l maint -d '`NAME@EPOCH` — maintenance reservation the request was trimmed to fit' -r
complete -c sinteractive -n "__fish_sinteractive_using_subcommand __job" -l mouse
complete -c sinteractive -n "__fish_sinteractive_using_subcommand __job" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand __attach" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand __popup" -s h -l help -d 'Print help'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "launch" -d 'Launch a new session (the default when no subcommand is given)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "attach" -d 'Reattach to a session by JOBID or NAME (your only session when omitted)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "list" -d 'List running sessions'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "status" -d 'Show one session\'s status'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "cancel" -d 'Cancel a session'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "queue" -d 'Your job queue: running, pending (with reasons), and recent history'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "monitor" -d 'Live CPU/GPU/process view of a session\'s node, or any host'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "quota" -d 'Storage quota (Bodhi quota daemons; unavailable on other clusters)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "doctor" -d 'Check this install and, optionally, every compute node'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "session" -d 'Drive a session from outside: ensure, peek, send, events'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "claude" -d 'Claude Code integration: install, context, hook, statusline, mcp'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "gen" -d 'Generated output: completions, man page, JSON schemas'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "ensure" -d 'Superseded by `session ensure`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "peek" -d 'Superseded by `session peek`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "send" -d 'Superseded by `session send`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "events" -d 'Superseded by `session events`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "refresh" -d 'Superseded by `status --refresh`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "snapshot" -d 'Superseded by `monitor --once`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "agent-context" -d 'Superseded by `claude context`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "hook" -d 'Superseded by `claude hook`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "statusline" -d 'Superseded by `claude statusline`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "mcp" -d 'Superseded by `claude mcp`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "install-claude" -d 'Superseded by `claude install`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "completions" -d 'Superseded by `gen completions`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "man" -d 'Superseded by `gen man`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "schema" -d 'Superseded by `gen schema`'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "__job" -d 'The batch job body: starts zellij on the node and babysits it'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "__attach" -d 'Runs on the node over ssh: attach the local zellij client'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "__popup" -d 'In-session floating views'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and not __fish_seen_subcommand_from launch attach list status cancel queue monitor quota doctor session claude gen ensure peek send events refresh snapshot agent-context hook statusline mcp install-claude completions man schema __job __attach __popup help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "ensure" -d 'Reuse the session named NAME, or launch it if absent (implies --detach)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "peek" -d 'Read the last lines of a session\'s screen'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "send" -d 'Type a command into a session\'s shell'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "events" -d 'Stream session events (NDJSON)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from claude" -f -a "install" -d 'Install the skills, hooks, statusline and MCP server'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from claude" -f -a "context" -d 'Brief a coding agent on the session it is running inside'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from claude" -f -a "hook" -d 'Hook entry points (Claude Code runs these)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from claude" -f -a "statusline" -d 'statusLine command (Claude Code runs this)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from claude" -f -a "mcp" -d 'MCP server over stdio (Claude Code runs this)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from gen" -f -a "completions" -d 'Shell completions'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from gen" -f -a "man" -d 'The man page (roff)'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from gen" -f -a "schema" -d 'The JSON schemas of the machine-readable outputs'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "session-start" -d 'SessionStart: print the agent briefing'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "prompt" -d 'UserPromptSubmit: warn when walltime is short'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "worktree-create" -d 'WorktreeCreate: make the worktree on the cluster\'s scratch filesystem'
complete -c sinteractive -n "__fish_sinteractive_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "worktree-remove" -d 'WorktreeRemove: remove a worktree the create hook made'
