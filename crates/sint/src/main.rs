// Used by __job/__attach/doctor, wired in phase 2.
#[allow(dead_code)]
mod bundle;
mod cli;
mod commands;

use clap::Parser;

use cli::{split_launch_argv, Cli, Command};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
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
