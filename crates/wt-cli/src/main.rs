use anyhow::{bail, Context as _, Result};
use clap::{Parser, Subcommand};
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use ssh_key::{HashAlg, PublicKey};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicBool, Ordering};
use wt_api::{ApiRequest, CreateInstance, Operation, Response};
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
    New,
    /// List worlds across every configured context.
    Ls,
    /// Remove a world.
    Rm { name: String },
    /// Open a world in VS Code Remote-SSH.
    Code { name: String },
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
        Command::New => {
            let input = prompt_create(&config)?;
            let context = config
                .context(&input.context)
                .context("selected context is missing")?;
            let spinner = cliclack::spinner();
            spinner.start("Creating world");
            let response = wt_cli::transport::call(
                context,
                &ApiRequest::new(Operation::Create(CreateInstance {
                    name: input.name.clone(),
                    source: input.source,
                    git_branch: input.git_branch,
                    git_ref: input.git_ref,
                    git_user_name: input.git_user_name,
                    git_user_email: input.git_user_email,
                    vcpus: input.vcpus,
                    memory_mib: input.memory_mib,
                    disk_gib: input.disk_gib,
                    ssh_authorized_keys: input.ssh_authorized_keys,
                })),
            );
            match &response {
                Ok(_) => spinner.stop("World created"),
                Err(_) => spinner.error("World creation did not complete"),
            }
            let response = response.map_err(|error| {
                anyhow::anyhow!(
                    "create did not complete; run `wt ls` to check the world: {error:#}"
                )
            })?;
            let Response::Instance { instance } = response else {
                bail!("helper returned the wrong response to create");
            };
            let instance = *instance;
            warn_if_sync_skipped(&config)?;
            println!(
                "{}.{}\t{}\t{}",
                context.name,
                instance.name,
                instance.status,
                instance.guest_ip.as_deref().unwrap_or("-")
            );
            let ssh = instance
                .ssh
                .as_ref()
                .context("created world has no SSH endpoint")?;
            println!("\nStarting setup: ssh {}.{}", context.name, instance.name);
            println!("Guest host: ssh {}.{}-host", context.name, instance.name);
            println!("Endpoint: {}@{}:{}", ssh.user, ssh.host, ssh.port);
            std::io::stdout().flush()?;
            let target = format!("{}.{}", context.name, instance.name);
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
        Command::Code { name } => open_in_code(&config, &name)?,
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

const DEFAULT_VCPUS: u32 = 2;
const DEFAULT_MEMORY_MIB: u64 = 4096;
const DEFAULT_DISK_GIB: u64 = 32;
static CANCELLED: AtomicBool = AtomicBool::new(false);

struct CreateInput {
    context: String,
    name: wt_api::InstanceName,
    source: String,
    git_branch: Option<String>,
    git_ref: Option<String>,
    vcpus: u32,
    memory_mib: u64,
    disk_gib: u64,
    ssh_authorized_keys: Vec<String>,
    git_user_name: String,
    git_user_email: String,
}

extern "C" fn cancel_prompt(_: i32) {
    CANCELLED.store(true, Ordering::SeqCst);
}

struct SignalGuard(Vec<(Signal, SigAction)>);

impl Drop for SignalGuard {
    fn drop(&mut self) {
        for (signal, action) in &self.0 {
            // SAFETY: restore the action returned by the matching sigaction call.
            let _ = unsafe { signal::sigaction(*signal, action) };
        }
    }
}

fn install_cancel_handlers() -> Result<SignalGuard> {
    CANCELLED.store(false, Ordering::SeqCst);
    let action = SigAction::new(
        SigHandler::Handler(cancel_prompt),
        SaFlags::empty(),
        SigSet::empty(),
    );
    let mut previous = Vec::new();
    for signal in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the handler only stores to a lock-free atomic.
        let old = unsafe { signal::sigaction(signal, &action) }
            .with_context(|| format!("install {signal} handler"))?;
        previous.push((signal, old));
    }
    Ok(SignalGuard(previous))
}

fn prompt_create(config: &ClientConfig) -> Result<CreateInput> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        bail!("`wt new` requires an interactive terminal");
    }
    let _signals = install_cancel_handlers()?;
    let git_author = read_git_author()?;
    cliclack::intro("Create a new world")?;
    let default_context = config
        .contexts
        .first()
        .context("no contexts are configured")?
        .name
        .clone();
    let context = if config.contexts.len() == 1 {
        default_context
    } else {
        let mut prompt = cliclack::select("Where should the world run?");
        for context in &config.contexts {
            prompt = prompt.item(context.name.clone(), &context.name, "");
        }
        prompt
            .initial_value(default_context)
            .filter_mode()
            .interact()
            .map_err(prompt_error)?
    };
    let name: String = cliclack::input("World name")
        .placeholder("my-world")
        .validate(|value: &String| {
            wt_api::InstanceName::parse(value.clone())
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .interact()
        .map_err(prompt_error)?;
    let name = wt_api::InstanceName::parse(name)?;
    let source: String = cliclack::input("Git repository")
        .placeholder("git@example.com:team/repository.git")
        .validate(|value: &String| {
            wt_api::validate_ssh_git_source(value).map_err(|error| error.to_string())
        })
        .interact()
        .map_err(prompt_error)?;
    let revision_value: String = cliclack::input("Revision")
        .default_input("default")
        .placeholder("default, branch:NAME, or ref:VALUE")
        .validate(|value: &String| parse_revision(value).map(|_| ()))
        .interact()
        .map_err(prompt_error)?;
    let revision = parse_revision(&revision_value)?;
    let vcpus = prompt_number("Virtual CPUs", DEFAULT_VCPUS)?;
    let memory_mib = prompt_number("RAM (MiB)", DEFAULT_MEMORY_MIB)?;
    let disk_gib = prompt_number("Disk (GiB)", DEFAULT_DISK_GIB)?;
    let keys = discover_public_keys()?;
    let revision_summary = match &revision {
        (Some(branch), None) => format!("branch:{branch}"),
        (None, Some(reference)) => format!("ref:{reference}"),
        _ => "default".to_owned(),
    };
    let mut summary = format!(
        "World       {name}\nContext     {context}\nRepository  {source}\nRevision    {revision_summary}\nGit author  {} <{}>\nResources   {vcpus} CPU · {memory_mib} MiB RAM · {disk_gib} GiB disk\nSSH keys    {}",
        git_author.name,
        git_author.email,
        keys.len()
    );
    for (_, fingerprint) in &keys {
        write!(summary, "\n            {fingerprint}")?;
    }
    cliclack::note("Review", summary)?;
    if !cliclack::confirm("Create this world?")
        .initial_value(true)
        .interact()
        .map_err(prompt_error)?
    {
        cliclack::outro_cancel("Creation cancelled")?;
        bail!("creation cancelled");
    }
    Ok(CreateInput {
        context,
        name,
        source,
        git_branch: revision.0,
        git_ref: revision.1,
        vcpus,
        memory_mib,
        disk_gib,
        ssh_authorized_keys: keys.into_iter().map(|(key, _)| key).collect(),
        git_user_name: git_author.name,
        git_user_email: git_author.email,
    })
}

fn prompt_error(error: std::io::Error) -> anyhow::Error {
    if error.kind() == std::io::ErrorKind::Interrupted || CANCELLED.load(Ordering::SeqCst) {
        anyhow::anyhow!("creation cancelled")
    } else {
        error.into()
    }
}

fn prompt_number<T>(label: &str, default: T) -> Result<T>
where
    T: std::str::FromStr + std::fmt::Display + Copy + PartialEq + Default + 'static,
    T::Err: std::fmt::Display,
{
    cliclack::input(label)
        .default_input(&default.to_string())
        .validate(|value: &String| match value.parse::<T>() {
            Ok(number) if number != T::default() => Ok(()),
            _ => Err("Enter a number greater than zero."),
        })
        .interact()
        .map_err(prompt_error)
}

fn parse_revision(value: &str) -> Result<(Option<String>, Option<String>)> {
    if value == "default" {
        return Ok((None, None));
    }
    if let Some(branch) = value
        .strip_prefix("branch:")
        .filter(|value| !value.is_empty())
    {
        return Ok((Some(branch.to_owned()), None));
    }
    if let Some(reference) = value.strip_prefix("ref:").filter(|value| !value.is_empty()) {
        return Ok((None, Some(reference.to_owned())));
    }
    bail!("Use default, branch:NAME, or ref:VALUE.")
}

fn discover_public_keys() -> Result<Vec<(String, String)>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let directory = home.join(".ssh");
    let entries = std::fs::read_dir(&directory)
        .with_context(|| format!("read SSH directory {}", directory.display()))?;
    let mut keys = BTreeSet::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("read {} entry", directory.display()))?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("pub")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let value = std::fs::read_to_string(entry.path())
            .with_context(|| format!("read public key {}", entry.path().display()))?;
        let mut key = PublicKey::from_openssh(value.trim())
            .with_context(|| format!("parse public key {}", entry.path().display()))?;
        key.set_comment("");
        keys.insert(key.to_openssh()?);
    }
    if keys.is_empty() {
        bail!("no valid public keys found in {}", directory.display());
    }
    keys.into_iter()
        .map(|key| {
            let parsed = PublicKey::from_openssh(&key)?;
            Ok((key, parsed.fingerprint(HashAlg::Sha256).to_string()))
        })
        .collect()
}

#[derive(Debug, serde::Deserialize)]
struct AppInfo {
    workspace: String,
}

fn open_in_code(config: &ClientConfig, target: &str) -> Result<()> {
    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        return Err(context_failures(
            "VS Code was not opened because the complete world list is unavailable",
            &report.failures,
            None,
        ));
    }
    let selected = inventory::resolve(&report.instances, target)?;
    if selected.instance.status != wt_api::InstanceStatus::Running {
        bail!(
            "world {} is {}; VS Code can only open a running world",
            selected.qualified_name(),
            selected.instance.status
        );
    }
    if selected.instance.ssh.is_none() || selected.instance.app_ssh.is_none() {
        bail!(
            "world {} has incomplete SSH access information",
            selected.qualified_name()
        );
    }

    wt_cli::ssh::sync(config, &report.instances)?;
    let qualified = selected.qualified_name();
    let workspace = discover_app_workspace(&qualified)?;
    launch_code(&qualified, &workspace)
}

fn discover_app_workspace(qualified: &str) -> Result<String> {
    let host = format!("{qualified}-host");
    let output = ProcessCommand::new("ssh")
        .args(["--", &host, "/usr/local/bin/wt-app-info"])
        .output()
        .with_context(|| format!("start OpenSSH to inspect {qualified}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if detail.is_empty() {
            bail!("inspect {qualified}: ssh exited with {}", output.status);
        }
        bail!(
            "inspect {qualified}: ssh exited with {}: {detail}",
            output.status
        );
    }
    let app: AppInfo = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decode app information for {qualified}"))?;
    if !Path::new(&app.workspace).is_absolute() {
        bail!("app workspace for {qualified} is not an absolute path");
    }
    Ok(app.workspace)
}

fn launch_code(qualified: &str, workspace: &str) -> Result<()> {
    let authority = format!("ssh-remote+{qualified}-vs");
    let status = ProcessCommand::new("code")
        .args(["--remote", &authority, workspace])
        .status()
        .context("start the VS Code command-line interface (`code`)")?;
    if !status.success() {
        bail!("VS Code exited with {status}");
    }
    Ok(())
}

#[derive(Debug)]
struct GitAuthor {
    name: String,
    email: String,
}

fn read_git_author() -> Result<GitAuthor> {
    Ok(GitAuthor {
        name: read_global_git_config("user.name")?,
        email: read_global_git_config("user.email")?,
    })
}

fn read_global_git_config(key: &str) -> Result<String> {
    match ProcessCommand::new("git")
        .args(["config", "--global", "--null", "--get", key])
        .output()
    {
        Ok(output) if output.status.success() => parse_git_config_value(&output.stdout)?
            .ok_or_else(|| required_git_config_error(key, None)),
        Ok(output) if output.status.code() == Some(1) => Err(required_git_config_error(key, None)),
        Ok(output) => Err(required_git_config_error(
            key,
            Some(String::from_utf8_lossy(&output.stderr).trim()),
        )),
        Err(error) => Err(required_git_config_error(key, Some(&error.to_string()))),
    }
}

fn required_git_config_error(key: &str, detail: Option<&str>) -> anyhow::Error {
    let detail = detail
        .filter(|detail| !detail.is_empty())
        .map(|detail| format!(": {detail}"))
        .unwrap_or_default();
    anyhow::anyhow!(
        "global Git {key} is required; configure it with `git config --global {key} VALUE`{detail}"
    )
}

fn parse_git_config_value(stdout: &[u8]) -> Result<Option<String>> {
    let value = stdout.strip_suffix(b"\0").unwrap_or(stdout);
    let value = std::str::from_utf8(value).map_err(|error| anyhow::anyhow!(error))?;
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

fn format_instances(instances: &[ContextInstance]) -> String {
    let mut rows = Vec::with_capacity(instances.len() + 1);
    rows.push([
        "CONTEXT".to_owned(),
        "NAME".to_owned(),
        "STATUS".to_owned(),
        "REPO".to_owned(),
        "RESOURCES".to_owned(),
        "DETAIL".to_owned(),
    ]);
    rows.extend(instances.iter().map(|item| {
        let instance = &item.instance;
        [
            item.context.clone(),
            instance.name.to_string(),
            instance.status.to_string(),
            wt_cli::ssh::repository_name(&instance.source)
                .unwrap_or("-")
                .to_owned(),
            format_resources(instance.vcpus, instance.memory_mib, instance.disk_gib),
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
            "{:<context_width$}  {:<name_width$}  {:<status_width$}  {:<repo_width$}  {:<resources_width$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            row[4],
            row[5],
            context_width = widths[0],
            name_width = widths[1],
            status_width = widths[2],
            repo_width = widths[3],
            resources_width = widths[4],
        )
        .expect("writing to a String cannot fail");
    }
    output
}

fn format_resources(vcpus: u32, memory_mib: u64, disk_gib: u64) -> String {
    let memory = if memory_mib.is_multiple_of(1024) {
        format!("{}G", memory_mib / 1024)
    } else {
        format!("{memory_mib}MiB")
    };
    format!("{vcpus} CPU · {memory} · {disk_gib}G")
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
    use std::sync::Mutex;

    static PROMPT_LOCK: Mutex<()> = Mutex::new(());
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
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
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
        running.instance.memory_mib = 1536;
        running.instance.guest_ip = Some("192.0.2.10".to_owned());
        running.instance.ssh = Some(SshAccess {
            user: "wt".to_owned(),
            host: "192.0.2.10".to_owned(),
            port: 2222,
            host_keys: Vec::new(),
        });

        let output = format_instances(&[provisioning, running]);

        insta::assert_snapshot!(output, @r###"
        CONTEXT     NAME          STATUS        REPO  RESOURCES              DETAIL
        local       jsdev-manual  provisioning  repo  2 CPU · 4G · 32G       -
        remote-lab  a             running       repo  2 CPU · 1536MiB · 32G  -
        "###);
        assert!(!output.contains('\t'));
    }

    #[test]
    fn formats_header_for_empty_inventory() {
        insta::assert_snapshot!(format_instances(&[]), @"CONTEXT  NAME  STATUS  REPO  RESOURCES  DETAIL");
    }

    #[test]
    fn formats_reconciliation_error_details() {
        let mut failed = item("local", "jsdev", InstanceStatus::Error);
        failed.instance.last_error = Some("SSH endpoint identity mismatch".to_owned());

        insta::assert_snapshot!(format_instances(&[failed]), @r###"
        CONTEXT  NAME   STATUS  REPO  RESOURCES         DETAIL
        local    jsdev  error   repo  2 CPU · 4G · 32G  SSH endpoint identity mismatch
        "###);
    }

    #[test]
    fn rejects_removed_ssh_subcommand() {
        assert!(Cli::try_parse_from(["wt", "ssh", "world"]).is_err());
    }

    #[test]
    fn parses_code_target() {
        let cli = Cli::try_parse_from(["wt", "code", "ars.jsdev"]).unwrap();
        let Command::Code { name } = cli.command else {
            panic!("expected code command");
        };
        assert_eq!(name, "ars.jsdev");
    }

    #[test]
    fn new_is_interactive_only() {
        assert!(matches!(
            Cli::try_parse_from(["wt", "new"]).unwrap().command,
            Command::New
        ));
        assert!(Cli::try_parse_from(["wt", "new", "git@example.test:repo.git"]).is_err());
        assert_eq!(
            parse_revision("branch:work").unwrap(),
            (Some("work".to_owned()), None)
        );
        assert_eq!(
            parse_revision("ref:0123456789abcdef").unwrap(),
            (None, Some("0123456789abcdef".to_owned()))
        );
        assert_eq!(parse_revision("default").unwrap(), (None, None));
    }

    #[test]
    fn prompt_cancels_after_a_signal() {
        let _lock = PROMPT_LOCK.lock().unwrap();
        CANCELLED.store(false, Ordering::SeqCst);
        cancel_prompt(0);
        let error = prompt_error(std::io::Error::other("prompt failed"));
        assert_eq!(error.to_string(), "creation cancelled");
        CANCELLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn parses_git_author_values_without_losing_spaces_or_unicode() {
        assert_eq!(
            parse_git_config_value("Lucas Ávila \0".as_bytes()).unwrap(),
            Some("Lucas Ávila ".to_owned())
        );
        assert_eq!(parse_git_config_value(b"\0").unwrap(), None);
    }

    #[test]
    fn explains_required_git_author_value() {
        insta::assert_snapshot!(
            required_git_config_error("user.email", None).to_string(),
            @"global Git user.email is required; configure it with `git config --global user.email VALUE`"
        );
    }
}
