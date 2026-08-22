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
            reconcile::reconcile()?;
            println!("Codex session index refreshed.");
        }
        Command::InstallConfig => install::install_user_config()?,
        Command::ReportHook => report::report_hook()?,
        Command::FocusPane {
            session_id,
            tmux_session,
            pane_id,
        } => println!("{}", focus::focus(session_id, &tmux_session, &pane_id)?),
    }
    Ok(())
}

fn run_trampoline(args: Vec<OsString>) -> Result<()> {
    let real_codex = install::real_codex()?;
    match reconcile::reconcile_with_codex(&real_codex) {
        Ok(()) => {}
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
  install-config  Install WT's exact Codex user configuration
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
"###
        );
    }
}
