use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use wt_api::{ApiRequest, CreateInstance, InstanceName, Operation, Response};
use wt_cli::config::{default_config_path, Config};

#[derive(Debug, Parser)]
#[command(name = "wt")]
struct Cli {
    #[arg(long, global = true)]
    context: Option<String>,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a world.
    New {
        source: String,
        name: InstanceName,
        #[arg(long = "ref")]
        git_ref: Option<String>,
    },
    /// List worlds.
    Ls,
    /// Remove a world.
    Rm { name: InstanceName },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or(default_config_path()?);
    let config = Config::load(&config_path)?;
    let context = config.select(cli.context.as_deref())?;

    match cli.command {
        Command::New {
            source,
            name,
            git_ref,
        } => {
            let response = wt_cli::transport::call(
                context,
                &ApiRequest::new(Operation::Create(CreateInstance {
                    source,
                    name,
                    git_ref,
                })),
            )?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to create");
            };
            println!(
                "{}\t{}\t{}",
                instance.name,
                instance.status,
                instance.guest_ip.as_deref().unwrap_or("-")
            );
        }
        Command::Ls => {
            let response = wt_cli::transport::call(context, &ApiRequest::new(Operation::List))?;
            let Response::Instances { instances } = response else {
                bail!("helper returned the wrong response to list");
            };
            println!("NAME\tSTATUS\tIP");
            for instance in instances {
                println!(
                    "{}\t{}\t{}",
                    instance.name,
                    instance.status,
                    instance.guest_ip.as_deref().unwrap_or("-")
                );
            }
        }
        Command::Rm { name } => {
            let response =
                wt_cli::transport::call(context, &ApiRequest::new(Operation::Delete { name }))?;
            let Response::Deleted { name } = response else {
                bail!("helper returned the wrong response to delete");
            };
            println!("removed {name}");
        }
    }
    Ok(())
}
