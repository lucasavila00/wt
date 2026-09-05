use anyhow::{bail, Context as _, Result};
#[cfg(test)]
use clap::CommandFactory as _;
use clap::{Parser, Subcommand};
use std::fmt::Write as _;
use std::io::Write;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use wt_client::config::ContextKind;
use wt_client::config::{ClientConfig, Context};
use wt_client::inventory::{self, ContextWorld};
use wt_client::transport::ContextError;
use wt_control_protocol::{ApiRequest, Operation, Response};

mod api;
mod code;
mod create;
mod git_author;
mod progress_toast;
mod reports;
mod shell;

const TEST_SERVER_WARNING: &str = "WARNING: WT E2E TEST SERVER — test fixtures are installed.";
#[cfg(test)]
const API_TYPESCRIPT_CONTRACT: &str = include_str!("../../../../../api/api.ts");
const API_HELP: &str = concat!(
    "CANONICAL TYPESCRIPT CONTRACT:\n\n",
    include_str!("../../../../../api/api.ts")
);
#[cfg(test)]
use git_author::{parse_git_config_value, required_git_config_error};

#[derive(Debug, Parser)]
#[command(name = "wt", version = wt_control_protocol::BUILD_DESCRIPTION)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a world.
    New,
    /// List worlds across every configured context.
    Ls,
    /// Remove a world.
    Rm { name: String },
    /// Change a world's name without changing its identity or disk.
    Rename { name: String, new_name: String },
    /// Start a stopped world.
    Start { name: String },
    /// Stop a guest.
    Stop { name: String },
    /// Open a world in VS Code Remote-SSH.
    Code { name: String },
    /// Synchronize SSH aliases and connect to a world.
    Ssh { name: String },
    /// Open the persistent world terminal workspace.
    Shell,
    /// Update managed OpenSSH inventory.
    Sync,
    /// Show reports submitted about wt-tools.
    Reports,
    /// Clear reports submitted about wt-tools.
    ClearReports,
    /// Read one versioned JSON API request from standard input.
    #[command(after_long_help = API_HELP)]
    Api,
    /// Show client and configured server build identities.
    Diagnostics,
}

fn main() {
    if let Err(error) = run_from(std::env::args_os().collect()) {
        eprintln!("wt: {error:#}");
        std::process::exit(1);
    }
}

fn run_from(args: Vec<std::ffi::OsString>) -> Result<()> {
    let command = Cli::parse_from(args).command;
    if matches!(command, Command::Api) {
        return api::run();
    }
    let config = ClientConfig::load()?;
    let test_server = local_test_server(&config);
    if test_server {
        eprintln!("{TEST_SERVER_WARNING}");
    }
    match command {
        Command::New => {
            let created = create::run(&config)?;
            let context = created.context;
            let world = created.world;
            println!(
                "{}.{}\t{}\t{}",
                context,
                world.name,
                world.status,
                world.guest_ip.as_deref().unwrap_or("-")
            );
            let ssh = world
                .ssh
                .as_ref()
                .context("created world has no SSH endpoint")?;
            println!("\nOpening: ssh {}.{}", context, world.name);
            println!("Direct: ssh {}.{}-direct", context, world.name);
            println!("Endpoint: {}@{}:{}", ssh.user, ssh.host, ssh.port);
            std::io::stdout().flush()?;
            let target = format!("{}.{}", context, world.name);
            return Err(ProcessCommand::new("ssh").arg(&target).exec())
                .with_context(|| format!("exec ssh {target}"));
        }
        Command::Ls => {
            let report = inventory::list_all(&config);
            if report.failures.len() == config.contexts.len() {
                return Err(context_failures(
                    "could not list worlds because every context failed",
                    &report.failures,
                    None,
                ));
            }
            print!("{}", format_worlds(&report.worlds));
            std::io::stdout().flush()?;
            if report.failures.is_empty() {
                wt_client::ssh::sync(&config, &report.worlds)?;
            } else {
                print_context_warnings(&report.failures);
                eprintln!(
                    "warning: SSH inventory was not updated because the complete world list is unavailable"
                );
            }
        }
        Command::Rm { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::GetWorld {
                    name: world_name.clone(),
                }),
            )?;
            let Response::World { world } = response else {
                bail!("helper returned the wrong response while resolving delete");
            };
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::DeleteWorld {
                    world_id: world.world_id,
                }),
            )?;
            let Response::WorldDeleted { .. } = response else {
                bail!("helper returned the wrong response to delete");
            };
            warn_if_sync_skipped(&config)?;
            println!("removed {}.{}", context.name, world_name);
        }
        Command::Rename { name, new_name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let old_name = world_name.clone();
            let new_name = wt_control_protocol::WorldName::parse(new_name)?;
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::GetWorld {
                    name: world_name.clone(),
                }),
            )?;
            let Response::World { world } = response else {
                bail!("helper returned the wrong response while resolving rename");
            };
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::RenameWorld {
                    world_id: world.world_id,
                    new_name: new_name.clone(),
                }),
            )?;
            let Response::World { world } = response else {
                bail!("helper returned the wrong response to rename");
            };
            warn_if_sync_skipped(&config)?;
            println!("renamed {}.{} to {}", context.name, old_name, world.name);
        }
        Command::Start { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let world = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::GetWorld {
                    name: world_name.clone(),
                }),
            )?;
            let Response::World { world } = world else {
                bail!("helper returned the wrong response while resolving start");
            };
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::StartWorld {
                    world_id: world.world_id,
                }),
            )?;
            let Response::World { world } = response else {
                bail!("helper returned the wrong response to start");
            };
            warn_if_sync_skipped(&config)?;
            println!("started {}.{} ({})", context.name, world_name, world.status);
        }
        Command::Stop { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let world = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::GetWorld {
                    name: world_name.clone(),
                }),
            )?;
            let Response::World { world } = world else {
                bail!("helper returned the wrong response while resolving stop");
            };
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::StopWorld {
                    world_id: world.world_id,
                }),
            )?;
            let Response::World { world } = response else {
                bail!("helper returned the wrong response to stop");
            };
            warn_if_sync_skipped(&config)?;
            println!("stopped {}.{} ({})", context.name, world_name, world.status);
        }
        Command::Code { name } => code::open(&config, &name)?,
        Command::Ssh { name } => wt_client::connection::ssh(&config, &name)?,
        Command::Shell => shell::run(&config, test_server)?,
        Command::Sync => {
            let path = sync_complete_inventory(&config)?;
            println!("updated {}", path.display());
        }
        Command::Reports => reports::show(&config)?,
        Command::ClearReports => reports::clear(&config)?,
        Command::Diagnostics => print_diagnostics(&config),
        Command::Api => unreachable!("API requests return before loading interactive client state"),
    }
    Ok(())
}

fn local_test_server(config: &ClientConfig) -> bool {
    config
        .contexts
        .iter()
        .filter(|context| matches!(context.kind, ContextKind::BareMetalLocal))
        .any(|context| {
            wt_client::transport::server_info(context)
                .map(|(test_server, _)| test_server)
                .unwrap_or(false)
        })
}

fn print_diagnostics(config: &ClientConfig) {
    let client = wt_control_protocol::BuildIdentity::current();
    println!("client\t{client}");
    for context in &config.contexts {
        match wt_client::transport::server_info(context) {
            Ok((_, server)) => {
                let status = build_status(&client, &server);
                println!("{}\t{}\t{status}", context.name, server);
            }
            Err(error) => println!("{}\tunavailable: {error}", context.name),
        }
    }
}

fn build_status(
    client: &wt_control_protocol::BuildIdentity,
    server: &wt_control_protocol::BuildIdentity,
) -> &'static str {
    if client == server {
        "match"
    } else {
        "MISMATCH"
    }
}

fn format_worlds(worlds: &[ContextWorld]) -> String {
    let mut rows = Vec::with_capacity(worlds.len() + 1);
    rows.push([
        "CONTEXT".to_owned(),
        "NAME".to_owned(),
        "STATUS".to_owned(),
        "RESOURCES".to_owned(),
        "DETAIL".to_owned(),
    ]);
    rows.extend(worlds.iter().map(|item| {
        let world = &item.world;
        [
            item.context.clone(),
            world.name.to_string(),
            world.status.to_string(),
            inventory::format_resources(world, item.disk_usage_bytes),
            inventory::format_detail(item),
        ]
    }));

    let mut widths = [0; 4];
    for row in &rows {
        for (width, value) in widths.iter_mut().zip(row) {
            *width = (*width).max(value.chars().count());
        }
    }

    let mut output = String::new();
    for row in rows {
        writeln!(
            output,
            "{:<context_width$}  {:<name_width$}  {:<status_width$}  {:<resources_width$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            row[4],
            context_width = widths[0],
            name_width = widths[1],
            status_width = widths[2],
            resources_width = widths[3],
        )
        .expect("writing to a String cannot fail");
    }
    output
}

fn required_context<'a>(config: &'a ClientConfig, name: &str) -> Result<&'a Context> {
    config
        .context(name)
        .ok_or_else(|| anyhow::anyhow!("unknown context: {name}"))
}

fn resolve_operation_target<'a>(
    config: &'a ClientConfig,
    target: &str,
) -> Result<(&'a Context, wt_control_protocol::WorldName)> {
    let (qualified_context, world_name) = inventory::parse_target(config, target)?;
    if let Some(context) = qualified_context {
        return Ok((context, world_name));
    }
    if config.contexts.len() == 1 {
        return Ok((&config.contexts[0], world_name));
    }

    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        return Err(context_failures(
            &format!("cannot safely resolve {target:?} while a context is unavailable"),
            &report.failures,
            Some("use a qualified name such as `context.world` to contact one context directly"),
        ));
    }
    let selected = inventory::resolve(&report.worlds, target)?;
    let context = required_context(config, &selected.context)?;
    Ok((context, selected.world.name.clone()))
}

fn warn_if_sync_skipped(config: &ClientConfig) -> Result<()> {
    let report = inventory::list_all(config);
    if report.failures.is_empty() {
        wt_client::ssh::sync(config, &report.worlds)?;
    } else {
        print_context_warnings(&report.failures);
        eprintln!(
            "warning: SSH inventory was not updated because the complete world list is unavailable"
        );
    }
    Ok(())
}

fn sync_complete_inventory(config: &ClientConfig) -> Result<PathBuf> {
    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        return Err(context_failures(
            "SSH inventory was not updated because the complete world list is unavailable",
            &report.failures,
            None,
        ));
    }
    wt_client::ssh::sync(config, &report.worlds)
}

fn print_context_warnings(failures: &[ContextError]) {
    for failure in failures {
        eprint!("{}", failure.diagnostic("warning"));
    }
}

fn context_failures(summary: &str, failures: &[ContextError], hint: Option<&str>) -> anyhow::Error {
    let mut message = summary.to_owned();
    for failure in failures {
        write!(message, "\n\n{}", failure.diagnostic("error").trim_end())
            .expect("writing to a String cannot fail");
    }
    if let Some(hint) = hint {
        write!(message, "\n\nhint: {hint}").expect("writing to a String cannot fail");
    }
    anyhow::Error::msg(message)
}

#[cfg(test)]
mod tests;
