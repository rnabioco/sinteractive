mod bundle;
mod cli;
mod commands;
mod zellij_cmd;
mod zellij_embed;

use clap::{ColorChoice, CommandFactory, FromArgMatches};
use sint_core::color::Palette;
use sint_core::config::ColorMode;

use cli::{split_launch_argv, Cli, Command};
use commands::common::eprint_error;

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
            eprint_error(&stderr_palette(), &msg);
            std::process::exit(2);
        }
    };
    let mut full = vec!["sinteractive".to_string()];
    full.extend(ours);
    // `SINTERACTIVE_COLOR` governs the help and usage colours too, not just
    // the palette the subcommands print with.
    let matches = Cli::command()
        .color(match ColorMode::from_env() {
            ColorMode::Always => ColorChoice::Always,
            ColorMode::Never => ColorChoice::Never,
            ColorMode::Auto => ColorChoice::Auto,
        })
        .get_matches_from(full);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let (mut command, deprecated) = cli.resolve();
    if deprecated {
        let p = stderr_palette();
        eprintln!(
            "{}{}sinteractive:{}{} top-level flags are deprecated; use subcommands (see {}sinteractive --help{}{})",
            p.warn, p.bold, p.reset, p.warn, p.key, p.warn, p.reset
        );
    }
    // The launch flags `split_launch_argv` kept for us belong to whichever
    // verb is doing the launching.
    let launching = match &mut command {
        Command::Launch(l) => Some(l),
        Command::Ensure(e) => Some(&mut e.launch),
        Command::Session(s) => match &mut s.command {
            cli::SessionCommand::Ensure(e) => Some(&mut e.launch),
            _ => None,
        },
        _ => None,
    };
    if let Some(l) = launching {
        l.sbatch_args = sbatch_args;
    }
    let code = match commands::dispatch(command) {
        Ok(code) => code,
        Err(e) => {
            eprint_error(&stderr_palette(), &format!("{e:#}"));
            1
        }
    };
    std::process::exit(code);
}

/// The narration palette, for the messages `main` itself prints. The
/// subcommands take theirs from `Ctx`, which is not built yet here.
fn stderr_palette() -> Palette {
    Palette::for_fd(ColorMode::from_env(), 2)
}
