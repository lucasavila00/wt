use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use wt_api::{ApiRequest, CreateInstance, InstanceName, Operation, Response};

#[derive(Debug, Parser)]
#[command(name = "wt")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a world.
    New { name: InstanceName },
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
    match Cli::parse().command {
        Command::New { name } => {
            let response =
                wt_cli::transport::call(&ApiRequest::new(Operation::Create(CreateInstance {
                    name,
                })))?;
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
            let response = wt_cli::transport::call(&ApiRequest::new(Operation::List))?;
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
            let response = wt_cli::transport::call(&ApiRequest::new(Operation::Delete { name }))?;
            let Response::Deleted { name } = response else {
                bail!("helper returned the wrong response to delete");
            };
            println!("removed {name}");
        }
    }
    Ok(())
}
