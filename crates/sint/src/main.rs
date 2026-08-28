// Fully used once __job/__attach land; doctor uses part of it now.
#[allow(dead_code)]
mod bundle;
mod cli;
mod commands;
// `command` and the socket helpers are __job/__attach's; peek/send use
// `remote_argv`.
#[allow(dead_code)]
mod zellij_cmd;
mod zellij_embed;

use clap::Parser;

use cli::{split_launch_argv, Cli, Command};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // zellij's client library starts its server by re-executing this binary
    // with `--server SOCKET`; hand that straight to the embedded zellij.
    if argv.first().map(String::as_str) == Some("--server") {
        let mut zargs = vec!["zellij".to_string()];
        zargs.extend(argv);
        zellij_embed::run(zargs);
    }
    // `sinteractive zellij ARGS…` is the full zellij CLI, in-process.
    if argv.first().map(String::as_str) == Some("zellij") {
        let mut zargs = vec!["zellij".to_string()];
        zargs.extend(argv.into_iter().skip(1));
        zellij_embed::run(zargs);
    }
    let (ours, sbatch_args) = match split_launch_argv(&argv) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("sinteractive: {msg}");
            std::process::exit(2);
        }
    };
    let mut full = vec!["sinteractive".to_string()];
    full.extend(ours);
    let cli = Cli::parse_from(full);
    let (mut command, deprecated) = cli.resolve();
    if deprecated {
        eprintln!(
            "sinteractive: top-level flags are deprecated; use subcommands (see sinteractive --help)"
        );
    }
    if let Command::Launch(l) | Command::Ensure(cli::EnsureArgs { launch: l, .. }) = &mut command {
        l.sbatch_args = sbatch_args;
    }
    let code = match commands::dispatch(command) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("sinteractive: {e:#}");
            1
        }
    };
    std::process::exit(code);
}
