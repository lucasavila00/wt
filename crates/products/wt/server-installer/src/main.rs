mod host;
mod image;
mod install_input;
mod server;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use wt_installer_support::SystemRunner;

#[derive(Debug, Parser)]
#[command(name = "wts", version = wt_control_protocol::BUILD_DESCRIPTION)]
struct Cli {
    #[command(subcommand)]
    command: SetupCommand,
}

#[derive(Debug, Subcommand)]
enum SetupCommand {
    /// Parse and validate an install input without changing the host.
    Validate {
        /// Path to the install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
    /// Validate that an install input is explicitly safe for destructive E2E setup.
    ValidateE2e {
        /// Path to the E2E install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
    /// Create the isolated non-production Codex fixture for KVM E2E.
    PrepareE2e {
        /// Path to the E2E install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
    /// Install a complete local wt server from this source checkout.
    Install {
        /// Path to the install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
    /// Build or verify the golden image from install input.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ImageCommand {
    Build {
        /// Path to the install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
    Rebuild {
        /// Path to the install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
    /// Verify the installed golden image and provenance without changing the host.
    Verify {
        /// Path to the install input TOML.
        #[arg(long)]
        config: PathBuf,
    },
}

#[allow(dead_code)]
fn main() {
    if let Err(error) = run_from(std::env::args_os()) {
        eprintln!("\n{}", failure_message(&error));
        std::process::exit(1);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn failure_message(error: &anyhow::Error) -> String {
    format!("WT server setup failed: {error:#}")
}

pub fn run_from(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
) -> Result<()> {
    let runner = SystemRunner;
    match Cli::parse_from(args).command {
        SetupCommand::Validate { config } => {
            server::validate(&config).context("configuration validation stopped")?;
            println!("Configuration is valid: {}", config.display());
        }
        SetupCommand::ValidateE2e { config } => {
            server::validate_e2e(&config).context("E2E configuration validation stopped")?;
            println!("E2E configuration is valid: {}", config.display());
        }
        SetupCommand::PrepareE2e { config } => {
            server::prepare_e2e(&config).context("E2E fixture preparation stopped")?;
            println!("Prepared isolated E2E fixture: {}", config.display());
        }
        SetupCommand::Install { config } => {
            server::install(&runner, &config).context("server installation stopped")?
        }
        SetupCommand::Image {
            command: ImageCommand::Build { config },
        } => server::image(&runner, &config, false).context("image preparation stopped")?,
        SetupCommand::Image {
            command: ImageCommand::Rebuild { config },
        } => server::image(&runner, &config, true).context("image preparation stopped")?,
        SetupCommand::Image {
            command: ImageCommand::Verify { config },
        } => server::verify_images(&runner, &config).context("image verification stopped")?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn failure_message_identifies_error_operation_and_cause() {
        let error = anyhow!("image package manifest must contain exactly nine packages")
            .context("server installation stopped");

        insta::assert_snapshot!(failure_message(&error), @"WT server setup failed: server installation stopped: image package manifest must contain exactly nine packages");
    }
}
