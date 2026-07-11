mod files;
mod host;
mod image;
mod runner;
mod server;
mod test_cache;

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
    /// Parse and validate a server config without changing the host.
    Validate {
        #[arg(long)]
        config: PathBuf,
    },
    /// Install a complete local wt server from this source checkout.
    Install {
        #[arg(long)]
        config: PathBuf,
    },
    /// Build or verify the configured golden image.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ImageCommand {
    Build {
        #[arg(long)]
        config: PathBuf,
    },
    Rebuild {
        #[arg(long)]
        config: PathBuf,
    },
    /// Build or verify the Docker image cache used by the KVM integration test.
    TestCache {
        #[command(subcommand)]
        command: TestCacheCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TestCacheCommand {
    Build {
        #[arg(long)]
        config: PathBuf,
    },
    Rebuild {
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
        SetupCommand::Image {
            command:
                ImageCommand::TestCache {
                    command: TestCacheCommand::Build { config },
                },
        } => server::test_cache(&runner, &config, false)?,
        SetupCommand::Image {
            command:
                ImageCommand::TestCache {
                    command: TestCacheCommand::Rebuild { config },
                },
        } => server::test_cache(&runner, &config, true)?,
    }
    Ok(())
}
