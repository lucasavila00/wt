use anyhow::{bail, Result};
use base64::Engine as _;
use clap::{Parser, Subcommand};
use std::fmt::Write as _;
use std::io::Write as _;
use wt_api::{ApiRequest, CreateInstance, ErrorCode, GitPassphrase, Operation, Outcome, Response};
use wt_cli::config::{ClientConfig, Context};
use wt_cli::inventory::{self, ContextInstance};
use wt_cli::transport::ContextError;

#[derive(Debug, Parser)]
#[command(name = "wt")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a devcontainer-ready world.
    New { source: String, name: String },
    /// List worlds across every configured context.
    Ls,
    /// Remove a world.
    Rm { name: String },
    /// Replay and follow a world's provisioning log.
    Logs { name: String },
    /// Update managed OpenSSH inventory.
    Sync,
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
        Command::New { source, name } => {
            wt_api::validate_ssh_git_source(&source)?;
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
            let response =
                create_with_passphrase_attempts(context, world_name, source, |prompt| {
                    rpassword::prompt_password(prompt).map_err(Into::into)
                })?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to create");
            };
            let instance = if instance.status == wt_api::InstanceStatus::Provisioning {
                follow_logs(context, &instance.name)?
            } else {
                *instance
            };
            warn_if_sync_skipped(&config)?;
            println!(
                "{}.{}\t{}\t{}",
                context.name,
                instance.name,
                instance.status,
                instance.guest_ip.as_deref().unwrap_or("-")
            );
            if let Some(ssh) = &instance.ssh {
                println!("\nApp shell: ssh {}.{}", context.name, instance.name);
                println!(
                    "Editor / raw app SSH: ssh {}.{}-vs (also {}.{}-vc)",
                    context.name, instance.name, context.name, instance.name
                );
                println!("Guest host: ssh {}.{}-host", context.name, instance.name);
                println!("Endpoint: {}@{}:{}", ssh.user, ssh.host, ssh.port);
            }
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
                wt_cli::ssh::sync(&config, &report.instances)?;
            } else {
                print_context_warnings(&report.failures);
                eprintln!(
                    "warning: SSH inventory was not updated because the complete world list is unavailable"
                );
            }
        }
        Command::Rm { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            let response = wt_cli::transport::call(
                context,
                &ApiRequest::new(Operation::Delete {
                    name: world_name.clone(),
                }),
            )?;
            let Response::Deleted { .. } = response else {
                bail!("helper returned the wrong response to delete");
            };
            warn_if_sync_skipped(&config)?;
            println!("removed {}.{}", context.name, world_name);
        }
        Command::Logs { name } => {
            let (context, world_name) = resolve_operation_target(&config, &name)?;
            follow_logs(context, &world_name)?;
        }
        Command::Sync => {
            let report = inventory::list_all(&config);
            if !report.failures.is_empty() {
                return Err(context_failures(
                    "SSH inventory was not updated because the complete world list is unavailable",
                    &report.failures,
                    None,
                ));
            }
            let path = wt_cli::ssh::sync(&config, &report.instances)?;
            println!("updated {}", path.display());
        }
    }
    Ok(())
}

fn create_with_passphrase_attempts(
    context: &Context,
    world_name: wt_api::InstanceName,
    source: String,
    mut prompt_password: impl FnMut(String) -> Result<String>,
) -> Result<Response> {
    const MAX_ATTEMPTS: usize = 3;

    eprintln!(
        "To clone {source} into {}.{world_name}, WT must unlock the Git SSH key configured on that context's server. This may differ from the SSH key your client uses to connect to the server.",
        context.name
    );

    for attempt in 1..=MAX_ATTEMPTS {
        let passphrase = prompt_password("Server Git SSH key passphrase: ".to_owned())?;
        if passphrase.is_empty() {
            if attempt == MAX_ATTEMPTS {
                bail!("Git key passphrase must not be empty");
            }
            let remaining = MAX_ATTEMPTS - attempt;
            eprintln!(
                "Git key passphrase must not be empty; {remaining} attempt{} remaining.",
                if remaining == 1 { "" } else { "s" }
            );
            continue;
        }
        let outcome = wt_cli::transport::call_outcome(
            context,
            &ApiRequest::new(Operation::Create(CreateInstance {
                name: world_name.clone(),
                source: source.clone(),
                git_passphrase: GitPassphrase::new(passphrase),
            })),
        )
        .map_err(|error| {
            anyhow::anyhow!(
                "create acknowledgement was not received; the outcome is unknown. Run `wt ls` or `wt logs {}.{}` to check: {error:#}",
                context.name,
                world_name
            )
        })?;
        match outcome {
            Outcome::Ok { response } => return Ok(*response),
            Outcome::Error { error }
                if error.code == ErrorCode::InvalidGitPassphrase && attempt < MAX_ATTEMPTS =>
            {
                let remaining = MAX_ATTEMPTS - attempt;
                eprintln!(
                    "{}; {remaining} attempt{} remaining.",
                    error.message,
                    if remaining == 1 { "" } else { "s" }
                );
            }
            Outcome::Error { error } => bail!(wt_cli::transport::format_api_error(&error)),
        }
    }
    unreachable!("the final passphrase attempt always returns")
}

fn follow_logs(context: &Context, name: &wt_api::InstanceName) -> Result<wt_api::Instance> {
    let mut offset = 0_u64;
    loop {
        let response = wt_cli::transport::call(
            context,
            &ApiRequest::new(Operation::Logs {
                name: name.clone(),
                offset,
            }),
        )?;
        let Response::Logs {
            chunk,
            next_offset,
            status,
            last_error,
        } = response
        else {
            bail!("helper returned the wrong response to logs");
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(chunk)
            .map_err(|error| anyhow::anyhow!("decode provisioning log: {error}"))?;
        std::io::stdout().write_all(&bytes)?;
        std::io::stdout().flush()?;
        offset = next_offset;
        if status == wt_api::InstanceStatus::Provisioning || !bytes.is_empty() {
            continue;
        }
        if status == wt_api::InstanceStatus::Error {
            bail!(
                "provisioning {}.{} failed: {}",
                context.name,
                name,
                last_error.as_deref().unwrap_or("unknown error")
            );
        }
        let response = wt_cli::transport::call(
            context,
            &ApiRequest::new(Operation::Get { name: name.clone() }),
        )?;
        let Response::Instance { instance } = response else {
            bail!("helper returned the wrong response to get");
        };
        if instance.status != wt_api::InstanceStatus::Running {
            bail!("world reached unexpected status: {}", instance.status);
        }
        return Ok(*instance);
    }
}

fn format_instances(instances: &[ContextInstance]) -> String {
    let mut rows = Vec::with_capacity(instances.len() + 1);
    rows.push([
        "CONTEXT".to_owned(),
        "NAME".to_owned(),
        "STATUS".to_owned(),
        "IP".to_owned(),
        "SSH".to_owned(),
        "DETAIL".to_owned(),
    ]);
    rows.extend(instances.iter().map(|item| {
        let instance = &item.instance;
        let target = instance
            .ssh
            .as_ref()
            .map(|ssh| format!("{}@{}:{}", ssh.user, ssh.host, ssh.port))
            .unwrap_or_else(|| "-".to_owned());
        [
            item.context.clone(),
            instance.name.to_string(),
            instance.status.to_string(),
            instance.guest_ip.as_deref().unwrap_or("-").to_owned(),
            target,
            instance.last_error.as_deref().unwrap_or("-").to_owned(),
        ]
    }));

    let mut widths = [0; 5];
    for row in &rows {
        for (width, value) in widths.iter_mut().zip(row) {
            *width = (*width).max(value.chars().count());
        }
    }

    let mut output = String::new();
    for row in rows {
        writeln!(
            output,
            "{:<context_width$}  {:<name_width$}  {:<status_width$}  {:<ip_width$}  {:<ssh_width$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            row[4],
            row[5],
            context_width = widths[0],
            name_width = widths[1],
            status_width = widths[2],
            ip_width = widths[3],
            ssh_width = widths[4],
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
) -> Result<(&'a Context, wt_api::InstanceName)> {
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
        wt_cli::ssh::sync(config, &report.instances)?;
    } else {
        print_context_warnings(&report.failures);
        eprintln!(
            "warning: SSH inventory was not updated because the complete world list is unavailable"
        );
    }
    Ok(())
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
mod tests {
    use super::*;
    use uuid::Uuid;
    use wt_api::{Instance, InstanceName, InstanceStatus, SshAccess};

    fn item(context: &str, name: &str, status: InstanceStatus) -> ContextInstance {
        ContextInstance {
            context: context.to_owned(),
            instance: Instance {
                id: Uuid::new_v4(),
                name: InstanceName::parse(name).unwrap(),
                owner: "tester".to_owned(),
                status,
                source: "git@example.test:repo.git".to_owned(),
                guest_ip: None,
                last_error: None,
                ssh: None,
                app_ssh: None,
            },
        }
    }

    #[test]
    fn formats_aligned_instance_columns_without_tabs() {
        let provisioning = item("local", "jsdev-manual", InstanceStatus::Provisioning);
        let mut running = item("remote-lab", "a", InstanceStatus::Running);
        running.instance.guest_ip = Some("192.0.2.10".to_owned());
        running.instance.ssh = Some(SshAccess {
            user: "wt".to_owned(),
            host: "192.0.2.10".to_owned(),
            port: 2222,
            host_keys: Vec::new(),
        });

        let output = format_instances(&[provisioning, running]);

        insta::assert_snapshot!(output, @r###"
        CONTEXT     NAME          STATUS        IP          SSH                 DETAIL
        local       jsdev-manual  provisioning  -           -                   -
        remote-lab  a             running       192.0.2.10  wt@192.0.2.10:2222  -
        "###);
        assert!(!output.contains('\t'));
    }

    #[test]
    fn formats_header_for_empty_inventory() {
        insta::assert_snapshot!(format_instances(&[]), @"CONTEXT  NAME  STATUS  IP  SSH  DETAIL");
    }

    #[test]
    fn formats_reconciliation_error_details() {
        let mut failed = item("local", "jsdev", InstanceStatus::Error);
        failed.instance.last_error = Some("SSH endpoint identity mismatch".to_owned());

        insta::assert_snapshot!(format_instances(&[failed]), @r###"
        CONTEXT  NAME   STATUS  IP  SSH  DETAIL
        local    jsdev  error   -   -    SSH endpoint identity mismatch
        "###);
    }

    #[test]
    fn rejects_removed_ssh_subcommand() {
        assert!(Cli::try_parse_from(["wt", "ssh", "world"]).is_err());
    }
}
