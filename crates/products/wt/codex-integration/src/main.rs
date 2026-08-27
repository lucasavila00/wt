mod focus;
mod install;
mod reconcile;
mod startup;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
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
    /// Synchronize shared Codex history.
    Reconcile,
    /// Install WT's Codex configuration.
    InstallConfig,
    /// Focus an observed Codex Byobu pane.
    #[command(hide = true)]
    FocusPane {
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
    if invoked_as_codex(&args) {
        return run_trampoline(args);
    }

    match Cli::parse_from(args).command {
        Command::Reconcile => {
            startup::reconcile_manual()?;
            println!("Codex history synchronized.");
        }
        Command::InstallConfig => install::install_user_config()?,
        Command::FocusPane {
            tmux_session,
            pane_id,
        } => println!("{}", focus::focus(&tmux_session, &pane_id)?),
    }
    Ok(())
}

fn run_trampoline(args: Vec<OsString>) -> Result<()> {
    let real_codex = real_codex()?;
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
  reconcile       Synchronize shared Codex history
  install-config  Install WT's Codex configuration
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
"###
        );
    }
}
