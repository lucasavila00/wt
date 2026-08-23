use anyhow::{bail, Context as _, Result};
use clap::{Parser, Subcommand};
use std::fmt::Write as _;
use std::io::Write;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use wt_client::config::ContextKind;
use wt_client::config::{ClientConfig, Context};
use wt_client::inventory::{self, ContextInstance};
use wt_client::transport::ContextError;
use wt_control_protocol::{ApiRequest, Operation, Response};

mod code;
mod create;
mod git_author;
mod progress_toast;
mod reports;
mod shell;

const TEST_SERVER_WARNING: &str = "WARNING: WT E2E TEST SERVER — test fixtures are installed.";
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
    /// Show client and configured server build identities.
    Diagnostics,
}

fn main() {
    if let Err(error) = run_from(std::env::args_os().collect()) {
        eprintln!("wt: {error:#}");
        std::process::exit(1);
    }
}

fn run_from(mut args: Vec<std::ffi::OsString>) -> Result<()> {
    #[cfg(feature = "guest")]
    if invoked_as(&args, "codex") {
        return wt_codex_integration::run(args);
    }
    #[cfg(feature = "guest")]
    if invoked_as(&args, "git-remote-wt-agent") {
        return wt_agent_tool_gateway::git_remote_command::run_from(
            args.drain(1..)
                .map(|arg| arg.into_string().map_err(|_| anyhow::anyhow!("Git helper arguments must be UTF-8")))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    #[cfg(feature = "host")]
    if subcommand_is(&args, "server") {
        args.remove(1);
        args[0] = "wt server".into();
        return wt_server::run_from(args);
    }
    #[cfg(feature = "host")]
    if subcommand_is(&args, "server-setup") {
        args.remove(1);
        args[0] = "wt server-setup".into();
        return wt_server_installer::run_from(args);
    }
    #[cfg(feature = "guest")]
    if subcommand_is(&args, "tools") {
        return wt_agent_tool_gateway::tools_command::run(
            args.drain(2..)
                .map(|arg| arg.into_string().map_err(|_| anyhow::anyhow!("tool arguments must be UTF-8")))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    #[cfg(feature = "guest")]
    if args.get(1).is_some_and(|arg| arg == "guest")
        && args.get(2).is_some_and(|arg| arg == "relay")
    {
        args.drain(1..3);
        args[0] = "wt guest relay".into();
        return wt_agent_tool_gateway::relay_command::run_from(args);
    }
    #[cfg(feature = "guest")]
    if subcommand_is(&args, "codex") {
        args.remove(1);
        args[0] = "wt codex".into();
        return wt_codex_integration::run(args);
    }

    let command = Cli::parse_from(args).command;
    let config = ClientConfig::load()?;
    let test_server = local_test_server(&config);
    if test_server {
        eprintln!("{TEST_SERVER_WARNING}");
    }
    match command {
        Command::New => {
            let created = create::run(&config)?;
            let context = created.context;
            let instance = created.instance;
            println!(
                "{}.{}\t{}\t{}",
                context,
                instance.name,
                instance.status,
                instance.guest_ip.as_deref().unwrap_or("-")
            );
            let ssh = instance
                .ssh
                .as_ref()
                .context("created world has no SSH endpoint")?;
            println!("\nOpening: ssh {}.{}", context, instance.name);
            println!("Direct: ssh {}.{}-direct", context, instance.name);
            println!("Endpoint: {}@{}:{}", ssh.user, ssh.host, ssh.port);
            std::io::stdout().flush()?;
            let target = format!("{}.{}", context, instance.name);
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
            print!("{}", format_instances(&report.instances));
            std::io::stdout().flush()?;
            if report.failures.is_empty() {
                wt_client::ssh::sync(&config, &report.instances)?;
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
                &ApiRequest::new(Operation::Get {
                    name: world_name.clone(),
                }),
            )?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response while resolving delete");
            };
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::Delete {
                    name: world_name.clone(),
                    expected_id: instance.id,
                }),
            )?;
            let Response::Deleted { .. } = response else {
                bail!("helper returned the wrong response to delete");
            };
            warn_if_sync_skipped(&config)?;
            println!("removed {}.{}", context.name, world_name);
        }
        Command::Start { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::Start {
                    name: world_name.clone(),
                }),
            )?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to start");
            };
            warn_if_sync_skipped(&config)?;
            println!(
                "started {}.{} ({})",
                context.name, world_name, instance.status
            );
        }
        Command::Stop { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let response = wt_client::transport::call(
                context,
                &ApiRequest::new(Operation::Stop {
                    name: world_name.clone(),
                }),
            )?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to stop");
            };
            warn_if_sync_skipped(&config)?;
            println!(
                "stopped {}.{} ({})",
                context.name, world_name, instance.status
            );
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
    }
    Ok(())
}

#[cfg(feature = "guest")]
fn invoked_as(args: &[std::ffi::OsString], name: &str) -> bool {
    args.first()
        .and_then(|arg| std::path::Path::new(arg).file_name())
        .is_some_and(|arg| arg == name)
}

#[allow(dead_code)]
fn subcommand_is(args: &[std::ffi::OsString], name: &str) -> bool {
    args.get(1).is_some_and(|arg| arg == name)
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

fn format_instances(instances: &[ContextInstance]) -> String {
    let mut rows = Vec::with_capacity(instances.len() + 1);
    rows.push([
        "CONTEXT".to_owned(),
        "NAME".to_owned(),
        "STATUS".to_owned(),
        "RESOURCES".to_owned(),
        "DETAIL".to_owned(),
    ]);
    rows.extend(instances.iter().map(|item| {
        let instance = &item.instance;
        [
            item.context.clone(),
            instance.name.to_string(),
            instance.status.to_string(),
            inventory::format_resources(instance, item.disk_usage_bytes),
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
) -> Result<(&'a Context, wt_control_protocol::InstanceName)> {
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
    let selected = inventory::resolve(&report.instances, target)?;
    let context = required_context(config, &selected.context)?;
    Ok((context, selected.instance.name.clone()))
}

fn warn_if_sync_skipped(config: &ClientConfig) -> Result<()> {
    let report = inventory::list_all(config);
    if report.failures.is_empty() {
        wt_client::ssh::sync(config, &report.instances)?;
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
    wt_client::ssh::sync(config, &report.instances)
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
