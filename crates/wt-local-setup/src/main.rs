mod files;
mod host;
mod image;
mod runner;
mod site;
mod test_cache;

use anyhow::Result;
use clap::{Parser, Subcommand};
use runner::SystemRunner;
use std::path::PathBuf;
use wt_libvirt::SiteConfig;

#[derive(Debug, Parser)]
#[command(name = "wt-local-setup")]
struct Cli {
    #[command(subcommand)]
    command: SetupCommand,
}

#[derive(Debug, Subcommand)]
enum SetupCommand {
    /// Parse and validate a site config without changing the host.
    Validate {
        #[arg(long)]
        config: PathBuf,
    },
    /// Install a complete local wt site from this source checkout.
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
        eprintln!("wt-local-setup: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let runner = SystemRunner;
    match Cli::parse().command {
        SetupCommand::Validate { config } => {
            SiteConfig::load_from(&config).map_err(anyhow::Error::msg)?;
            println!("valid {}", config.display());
        }
        SetupCommand::Install { config } => site::install(&runner, &config)?,
        SetupCommand::Image {
            command: ImageCommand::Build { config },
        } => site::image(&runner, &config, false)?,
        SetupCommand::Image {
            command: ImageCommand::Rebuild { config },
        } => site::image(&runner, &config, true)?,
        SetupCommand::Image {
            command:
                ImageCommand::TestCache {
                    command: TestCacheCommand::Build { config },
                },
        } => site::test_cache(&runner, &config, false)?,
        SetupCommand::Image {
            command:
                ImageCommand::TestCache {
                    command: TestCacheCommand::Rebuild { config },
                },
        } => site::test_cache(&runner, &config, true)?,
    }
    Ok(())
}
