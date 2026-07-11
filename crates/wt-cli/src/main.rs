use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::process::Command as ProcessCommand;
use wt_api::{ApiRequest, CreateInstance, Operation, Response};
use wt_cli::config::{ClientConfig, Context};
use wt_cli::inventory::{self, ContextInstance};

#[derive(Debug, Parser)]
#[command(name = "wt")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a devcontainer-ready world.
    New {
        source: String,
        name: String,
        #[arg(long = "ref")]
        git_ref: Option<String>,
    },
    /// List worlds across every configured context.
    Ls,
    /// Remove a world.
    Rm { name: String },
    /// Update managed OpenSSH inventory.
    Sync,
    /// Enter a world through stock OpenSSH.
    Ssh { name: String },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = ClientConfig::load()?;
    match Cli::parse().command {
        Command::New {
            source,
            name,
            git_ref,
        } => {
            wt_api::validate_ssh_git_source(&source)?;
            if git_ref.as_deref().is_some_and(str::is_empty) {
                bail!("--ref must not be empty");
            }
            let (qualified_context, world_name) = inventory::parse_target(&config, &name)?;
            let context = match qualified_context {
                Some(context) => context,
                None if config.contexts.len() == 1 => &config.contexts[0],
                None => bail!(
                    "world context is ambiguous; use one of: {}",
                    config
                        .contexts
                        .iter()
                        .map(|context| format!("{}.{}", context.name, world_name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            let response = wt_cli::transport::call(
                context,
                &ApiRequest::new(Operation::Create(CreateInstance {
                    name: world_name,
                    source,
                    git_ref,
                })),
            )?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to create");
            };
            if let Err(error) = sync_inventory(&config) {
                bail!(
                    "created {}.{} but SSH inventory was not changed: {error:#}",
                    context.name,
                    instance.name
                );
            }
            println!(
                "{}.{}\t{}\t{}",
                context.name,
                instance.name,
                instance.status,
                instance.guest_ip.as_deref().unwrap_or("-")
            );
            if let Some(ssh) = &instance.ssh {
                println!("\nApp shell: ssh {}.{}", context.name, instance.name);
                println!("Guest host: ssh {}.{}-host", context.name, instance.name);
                println!("Endpoint: {}@{}:{}", ssh.user, ssh.host, ssh.port);
            }
        }
        Command::Ls => {
            let instances = inventory::list_all(&config)?;
            wt_cli::ssh::sync(&instances)?;
            println!("CONTEXT\tNAME\tSTATUS\tIP\tSSH");
            for item in instances {
                let instance = item.instance;
                let target = instance
                    .ssh
                    .as_ref()
                    .map(|ssh| format!("{}@{}:{}", ssh.user, ssh.host, ssh.port))
                    .unwrap_or_else(|| "-".to_owned());
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    item.context,
                    instance.name,
                    instance.status,
                    instance.guest_ip.as_deref().unwrap_or("-"),
                    target
                );
            }
        }
        Command::Rm { name } => {
            let instances = inventory::list_all(&config)?;
            let selected = inventory::resolve(&instances, &name)?;
            let context = required_context(&config, &selected.context)?;
            let world_name = selected.instance.name.clone();
            let response = wt_cli::transport::call(
                context,
                &ApiRequest::new(Operation::Delete {
                    name: world_name.clone(),
                }),
            )?;
            let Response::Deleted { .. } = response else {
                bail!("helper returned the wrong response to delete");
            };
            if let Err(error) = sync_inventory(&config) {
                bail!(
                    "removed {}.{} but SSH inventory was not changed: {error:#}",
                    context.name,
                    world_name
                );
            }
            println!("removed {}.{}", context.name, world_name);
        }
        Command::Sync => {
            let path = sync_inventory(&config)?;
            println!("updated {}", path.display());
        }
        Command::Ssh { name } => {
            let instances = inventory::list_all(&config)?;
            let selected = inventory::resolve(&instances, &name)?;
            if selected.instance.status != wt_api::InstanceStatus::Running {
                bail!("world is not running: {}", selected.qualified_name());
            }
            if selected.instance.ssh.is_none() {
                bail!("world has no SSH access: {}", selected.qualified_name());
            }
            wt_cli::ssh::sync(&instances)?;
            let alias = if name.contains('.') {
                selected.qualified_name()
            } else {
                selected.instance.name.to_string()
            };
            let status = ProcessCommand::new("ssh").arg(alias).status()?;
            if !status.success() {
                bail!("ssh exited with {status}");
            }
        }
    }
    Ok(())
}

fn required_context<'a>(config: &'a ClientConfig, name: &str) -> Result<&'a Context> {
    config
        .context(name)
        .ok_or_else(|| anyhow::anyhow!("unknown context: {name}"))
}

fn sync_inventory(config: &ClientConfig) -> Result<std::path::PathBuf> {
    let instances: Vec<ContextInstance> = inventory::list_all(config)?;
    wt_cli::ssh::sync(&instances)
}
