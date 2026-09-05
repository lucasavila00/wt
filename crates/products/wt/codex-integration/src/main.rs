mod app_server;
mod completion;
mod focus;
mod install;
mod runtime;
mod tracking;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

const REAL_CODEX_SUFFIX: &str = ".codex/packages/standalone/current/bin/codex";

#[derive(Debug, Parser)]
#[command(name = "wt-codex-integration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Install WT's Codex configuration.
    InstallConfig,
    /// Focus an observed Codex Byobu pane.
    #[command(hide = true)]
    FocusPane {
        tmux_session: String,
        pane_id: String,
    },
    /// Start a managed Codex thread and its visible Byobu window.
    #[command(hide = true)]
    RuntimeStart,
    /// Inspect a managed Codex thread and its visible Byobu window.
    #[command(hide = true)]
    RuntimeInspect { thread_id: String },
    /// Resume a persisted Codex thread and reopen its visible window if needed.
    #[command(hide = true)]
    RuntimeResume { thread_id: String },
    /// Start the next turn; reject a busy thread without steering it.
    #[command(hide = true)]
    RuntimeSend { thread_id: String },
    #[command(hide = true)]
    RuntimeSteer { thread_id: String, turn_id: String },
    #[command(hide = true)]
    RuntimeInterrupt { thread_id: String, turn_id: String },
    /// Reconcile tracked Codex threads and retry durable completion deliveries.
    #[command(hide = true)]
    WatchTurns,
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
    if invoked_as_codex(&args) {
        return run_trampoline(args);
    }

    match Cli::parse_from(args).command {
        Command::InstallConfig => install::install_user_config()?,
        Command::FocusPane {
            tmux_session,
            pane_id,
        } => println!("{}", focus::focus(&tmux_session, &pane_id)?),
        Command::RuntimeStart => {
            let message = read_stdin()?;
            print_json(&runtime::start(&message)?)?;
        }
        Command::RuntimeInspect { thread_id } => {
            print_json(&runtime::inspect(&thread_id)?)?;
        }
        Command::RuntimeResume { thread_id } => {
            print_json(&runtime::resume(&thread_id)?)?;
        }
        Command::RuntimeSend { thread_id } => {
            let message = read_stdin()?;
            print_json(&runtime::send(&thread_id, &message)?)?;
        }
        Command::WatchTurns => tracking::watch()?,
        Command::RuntimeSteer { thread_id, turn_id } => {
            print_json(&runtime::control_turn(
                &thread_id,
                &turn_id,
                Some(&read_stdin()?),
            )?)?;
        }
        Command::RuntimeInterrupt { thread_id, turn_id } => {
            print_json(&runtime::control_turn(&thread_id, &turn_id, None)?)?;
        }
    }
    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut value = String::new();
    std::io::stdin()
        .read_to_string(&mut value)
        .context("read Codex message")?;
    if value.is_empty() {
        anyhow::bail!("Codex message is empty")
    }
    Ok(value)
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer(std::io::stdout().lock(), value).context("encode runtime response")?;
    println!();
    Ok(())
}

fn run_trampoline(args: Vec<OsString>) -> Result<()> {
    let real_codex = real_codex()?;
    let argv0 = args.first().context("missing process name")?;
    let error = ProcessCommand::new(&real_codex)
        .arg0(argv0)
        .args(&args[1..])
        .exec();
    Err(error).context("start the real Codex CLI")
}

fn invoked_as_codex(args: &[OsString]) -> bool {
    args.first().is_some_and(|argv0| {
        Path::new(argv0)
            .file_name()
            .is_some_and(|name| name == "codex")
    })
}

fn real_codex() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    let path = Path::new(&home).join(REAL_CODEX_SUFFIX);
    match path.metadata() {
        Ok(metadata) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 => Ok(path),
        Ok(_) => anyhow::bail!(
            "real Codex CLI is missing or not executable at {}; recreate this world from a verified WT image",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
            "real Codex CLI is missing or not executable at {}; recreate this world from a verified WT image",
            path.display()
        ),
        Err(error) => Err(error).with_context(|| format!("inspect real Codex CLI {}", path.display())),
    }
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
  install-config  Install WT's Codex configuration
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
"###
        );
    }
}
