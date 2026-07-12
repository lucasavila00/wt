mod files;
mod host;
mod image;
mod install_input;
mod registry_cache;
mod runner;
mod server;

use anyhow::Result;
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
        eprintln!("wt-server-setup: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let runner = SystemRunner;
    match Cli::parse().command {
        SetupCommand::Validate { config } => {
            server::validate(&config)?;
            println!("valid {}", config.display());
        }
        SetupCommand::Install { config } => server::install(&runner, &config)?,
        SetupCommand::Image {
            command: ImageCommand::Build { config },
        } => server::image(&runner, &config, false)?,
        SetupCommand::Image {
            command: ImageCommand::Rebuild { config },
        } => server::image(&runner, &config, true)?,
    }
    Ok(())
}
