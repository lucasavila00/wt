use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use wt_api::{ApiRequest, CreateInstance, InstanceName, Operation, Response};

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
        name: InstanceName,
        #[arg(long = "ref")]
        git_ref: Option<String>,
        #[arg(long)]
        identity: Option<PathBuf>,
    },
    /// List worlds.
    Ls,
    /// Remove a world.
    Rm { name: InstanceName },
    /// Update managed OpenSSH inventory.
    Sync,
    /// Enter a world through stock OpenSSH.
    Ssh { name: InstanceName },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::New { source, name, git_ref, identity } => {
            if source.trim().is_empty() { bail!("source must not be empty"); }
            if git_ref.as_deref().is_some_and(str::is_empty) { bail!("--ref must not be empty"); }
            let response = wt_cli::transport::call(&ApiRequest::new(Operation::Create(
                CreateInstance {
                    name,
                    source,
                    git_ref,
                    identity_file: identity.map(|path| path.to_string_lossy().into_owned()),
                },
            )));
            let sync = sync_inventory();
            let response = response?;
            sync?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to create");
            };
            sync_inventory()?;
            println!("{}\t{}\t{}", instance.name, instance.status, instance.guest_ip.as_deref().unwrap_or("-"));
            if let Some(ssh) = &instance.ssh {
                println!("\nHost {}", instance.name);
                println!("  HostName {}", ssh.host);
                println!("  User {}", ssh.user);
                println!("  Port {}", ssh.port);
            }
        }
        Command::Ls => {
            let instances = list_and_sync()?;
            println!("NAME\tSTATUS\tIP\tSSH");
            for instance in instances {
                let target = instance.ssh.as_ref()
                    .map(|ssh| format!("{}@{}:{}", ssh.user, ssh.host, ssh.port))
                    .unwrap_or_else(|| "-".to_owned());
                println!("{}\t{}\t{}\t{}", instance.name, instance.status,
                    instance.guest_ip.as_deref().unwrap_or("-"), target);
            }
        }
        Command::Rm { name } => {
            let response = wt_cli::transport::call(&ApiRequest::new(Operation::Delete { name }));
            let sync = sync_inventory();
            let response = response?;
            sync?;
            let Response::Deleted { name } = response else {
                bail!("helper returned the wrong response to delete");
            };
            println!("removed {name}");
        }
        Command::Sync => {
            let path = sync_inventory()?;
            println!("updated {}", path.display());
        }
        Command::Ssh { name } => {
            let instances = list_and_sync()?;
            if !instances.iter().any(|instance| {
                instance.name == name && instance.status == wt_api::InstanceStatus::Running
            }) {
                bail!("running instance not found: {name}");
            }
            let status = ProcessCommand::new("ssh").arg(name.as_str()).status()?;
            if !status.success() { bail!("ssh exited with {status}"); }
        }
    }
    Ok(())
}

fn list_instances() -> Result<Vec<wt_api::Instance>> {
    let response = wt_cli::transport::call(&ApiRequest::new(Operation::List))?;
    let Response::Instances { instances } = response else {
        bail!("helper returned the wrong response to list");
    };
    Ok(instances)
}

fn sync_inventory() -> Result<PathBuf> {
    wt_cli::ssh::sync(&list_instances()?)
}

fn list_and_sync() -> Result<Vec<wt_api::Instance>> {
    let instances = list_instances()?;
    wt_cli::ssh::sync(&instances)?;
    Ok(instances)
}
