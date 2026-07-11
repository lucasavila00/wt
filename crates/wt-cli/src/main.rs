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
    /// Rewrite the managed SSH config.
    Sync {
        #[arg(long)]
        output: Option<PathBuf>,
    },
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
            if let Some(block) = wt_cli::ssh::host_block(&instance) {
                print!("{block}");
            } else {
                println!("{}\t{}", instance.name, instance.status);
            }
        }
        Command::Ls => {
            let response = wt_cli::transport::call(context, &ApiRequest::new(Operation::List))?;
            let Response::Instances { instances } = response else {
                bail!("helper returned the wrong response to list");
            };
            println!("NAME\tSTATUS\tSSH");
            for instance in instances {
                let endpoint = instance
                    .endpoint
                    .map(|value| format!("{}@{}:{}", value.user, value.host, value.port))
                    .unwrap_or_else(|| "-".to_owned());
                println!("{}\t{}\t{}", instance.name, instance.status, endpoint);
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
        Command::Sync { output } => {
            let response = wt_cli::transport::call(context, &ApiRequest::new(Operation::List))?;
            let Response::Instances { instances } = response else {
                bail!("helper returned the wrong response to list");
            };
            let path = output.unwrap_or(wt_cli::ssh::default_ssh_config_path()?);
            wt_cli::ssh::sync(&path, &instances)?;
            println!("updated {}", path.display());
        }
    }
    Ok(())
}
