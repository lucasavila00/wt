mod focus;
mod install;
mod reconcile;
mod report;

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
    /// Install WT's exact Codex user configuration.
    InstallConfig,
    /// Report a WT-managed Codex lifecycle hook.
    #[command(hide = true)]
    ReportHook,
    /// Focus a strictly identified WT Byobu pane.
    #[command(hide = true)]
    FocusPane {
        session_id: uuid::Uuid,
        tmux_session: String,
        pane_id: String,
    },
    /// Restore the Codex command replaced by `install`.
    Uninstall,
}

fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    let silent = args.get(1).is_some_and(|value| value == "report-hook");
    if let Err(error) = run(args) {
        if silent {
            return;
        }
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
        Command::InstallConfig => install::install_user_config()?,
        Command::ReportHook => report::report_hook()?,
        Command::FocusPane {
            session_id,
            tmux_session,
            pane_id,
        } => println!("{}", focus::focus(session_id, &tmux_session, &pane_id)?),
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
  reconcile       Ask Codex to discover shared session rollouts
  install         Replace the Codex command in PATH with the WT trampoline
  install-config  Install WT's exact Codex user configuration
  uninstall       Restore the Codex command replaced by `install`
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
"###
        );
    }
}
