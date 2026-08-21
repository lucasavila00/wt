mod install;
mod reconcile;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command as ProcessCommand;

#[derive(Debug, Parser)]
#[command(name = "wt-codex-integration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Ask Codex to discover shared session rollouts.
    Reconcile,
    /// Replace the Codex command in PATH with the WT trampoline.
    Install,
    /// Restore the Codex command replaced by `install`.
    Uninstall,
}

fn main() {
    if let Err(error) = run(std::env::args_os().collect()) {
        eprintln!("wt-codex-integration: {error:#}");
        std::process::exit(1);
    }
}

fn run(args: Vec<OsString>) -> Result<()> {
    if install::invoked_as_codex(&args)? {
        return run_trampoline(args);
    }

    match Cli::parse_from(args).command {
        Command::Reconcile => {
            let result = reconcile::reconcile()?;
            for warning in &result.warnings {
                eprintln!("wt-codex-integration: {warning}");
            }
            println!(
                "Codex sessions: {} already indexed, {} reconciled.",
                result.already_indexed, result.reconciled
            );
        }
        Command::Install => {
            let outcome = install::install()?;
            println!("{}", outcome.message());
        }
        Command::Uninstall => {
            let path = install::uninstall()?;
            println!("Removed Codex trampoline: {}", path.display());
        }
    }
    Ok(())
}

fn run_trampoline(args: Vec<OsString>) -> Result<()> {
    let real_codex = install::active_installation(&args)?;
    match reconcile::reconcile_with_codex(&real_codex) {
        Ok(result) => {
            for warning in result.warnings {
                eprintln!("wt-codex-integration: {warning}");
            }
        }
        Err(error) => eprintln!("wt-codex-integration: reconciliation failed: {error:#}"),
    }

    let argv0 = args.first().context("missing process name")?;
    let error = ProcessCommand::new(&real_codex)
        .arg0(argv0)
        .args(&args[1..])
        .exec();
    Err(error).context("start the real Codex CLI")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_lists_the_complete_command_surface() {
        insta::assert_snapshot!(
            Cli::command().render_long_help().to_string(),
            @r###"
Usage: wt-codex-integration <COMMAND>

Commands:
  reconcile  Ask Codex to discover shared session rollouts
  install    Replace the Codex command in PATH with the WT trampoline
  uninstall  Restore the Codex command replaced by `install`
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
"###
        );
    }
}
