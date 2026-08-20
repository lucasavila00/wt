use anyhow::Result;
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "wt-git-proxy")]
struct Cli {
    #[arg(long, global = true, default_value = "/etc/wt-git-proxy/config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve,
    Tui,
}

pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let cli = Cli::parse_from(args);
    match cli.command {
        Command::Serve => crate::serve(&cli.config),
        Command::Tui => crate::run_tui(&cli.config),
    }
}
