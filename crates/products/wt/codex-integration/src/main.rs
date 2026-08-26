mod focus;
mod install;
mod reconcile;
mod startup;

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
    /// Focus a strictly identified WT Byobu pane.
    #[command(hide = true)]
    FocusPane {
        session_id: uuid::Uuid,
        tmux_session: String,
        pane_id: String,
    },
}

#[allow(dead_code)]
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    if let Err(error) = run(args) {
        eprintln!("wt-codex-integration: {error:#}");
        std::process::exit(1);
    }
}

pub fn run(args: Vec<OsString>) -> Result<()> {
    if install::invoked_as_codex(&args)? {
        return run_trampoline(args);
    }

    match Cli::parse_from(args).command {
        Command::Reconcile => {
            startup::reconcile_manual()?;
            println!("Codex session index refreshed.");
        }
        Command::InstallConfig => install::install_user_config()?,
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
    if std::env::var("IGNORE_CODEX_WT_CHECKS").as_deref() != Ok("true")
        && !is_version_request(&args)
    {
        println!(
            "Syncing shared Codex history before starting Codex. Set IGNORE_CODEX_WT_CHECKS=true to skip this synchronization."
        );
        startup::reconcile_before_start(&real_codex)?;
    }

    let argv0 = args.first().context("missing process name")?;
    let error = ProcessCommand::new(&real_codex)
        .arg0(argv0)
        .args(&args[1..])
        .exec();
    Err(error).context("start the real Codex CLI")
}

fn is_version_request(args: &[OsString]) -> bool {
    args.len() == 2 && matches!(args[1].to_str(), Some("--version" | "-V"))
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
