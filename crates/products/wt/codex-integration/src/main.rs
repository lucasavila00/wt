mod focus;
mod install;
mod reconcile;
mod report;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

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
        Err(error) => {
            let diagnostic = format!("{error:#}");
            match record_reconciliation_failure(&diagnostic) {
                Ok(path) => eprintln!(
                    "wt-codex-integration: reconciliation failed: {diagnostic}; full diagnostic recorded at {}",
                    path.display()
                ),
                Err(log_error) => eprintln!(
                    "wt-codex-integration: reconciliation failed: {diagnostic}; could not record diagnostic: {log_error:#}"
                ),
            }
        }
    }

    let argv0 = args.first().context("missing process name")?;
    let error = ProcessCommand::new(&real_codex)
        .arg0(argv0)
        .args(&args[1..])
        .exec();
    Err(error).context("start the real Codex CLI")
}

fn record_reconciliation_failure(diagnostic: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    let directory = PathBuf::from(home).join(".local/state/wt");
    fs::create_dir_all(&directory)
        .with_context(|| format!("create diagnostic directory {}", directory.display()))?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("secure diagnostic directory {}", directory.display()))?;
    let path = directory.join("codex-reconciliation.log");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time predates the Unix epoch")?
        .as_secs();
    let record = format!(
        "timestamp_unix={timestamp} pid={}\n{diagnostic}\n\n",
        std::process::id()
    );
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("open diagnostic log {}", path.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .with_context(|| format!("secure diagnostic log {}", path.display()))?;
    file.write_all(record.as_bytes())
        .with_context(|| format!("write diagnostic log {}", path.display()))?;
    Ok(path)
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
