mod files;
mod host;
mod image;
mod install_input;
mod registry_cache;
mod runner;
mod server;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use runner::SystemRunner;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "wt-server-setup")]
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
}

fn main() {
    if let Err(error) = run() {
        eprintln!("\n{}", failure_message(&error));
        std::process::exit(1);
    }
}

fn failure_message(error: &anyhow::Error) -> String {
    format!("ERROR: wt-server-setup: {error:#}")
}

fn run() -> Result<()> {
    let runner = SystemRunner;
    match Cli::parse().command {
        SetupCommand::Validate { config } => {
            server::validate(&config).context("configuration validation stopped")?;
            println!("valid {}", config.display());
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

        insta::assert_snapshot!(failure_message(&error), @"ERROR: wt-server-setup: server installation stopped: image package manifest must contain exactly nine packages");
    }
}
