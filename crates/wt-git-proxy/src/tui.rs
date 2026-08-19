use crate::{add_generated_key, add_public_key, list_keys, remove_key, ClientConfig, ProxyConfig};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Eq, PartialEq)]
enum Action { Upstream, Policy, AddClient, RemoveClient, Exit }

pub fn run_tui(config_path: &Path) -> Result<()> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        bail!("wt-git-proxy tui requires an interactive terminal");
    }
    cliclack::intro("Configure WT Git proxy")?;
    let mut config = if config_path.exists() { ProxyConfig::load(config_path)? } else {
        let config = ProxyConfig { write_prefix: input("Required write prefix", "refs/heads/agents/")?, allowed_branches: branch_list()? };
        config.save(config_path)?;
        config
    };
    loop {
        match cliclack::select("What do you want to do?")
            .item(Action::Upstream, "Configure upstream", "")
            .item(Action::Policy, "Set write policy", "")
            .item(Action::AddClient, "Authorize client", "")
            .item(Action::RemoveClient, "Revoke client", "")
            .item(Action::Exit, "Exit", "").interact()? {
            Action::Upstream => configure_upstream(config_path)?,
            Action::Policy => set_policy(config_path, &mut config)?,
            Action::AddClient => add_client(config_path)?,
            Action::RemoveClient => remove_client(config_path)?,
            Action::Exit => break,
        }
    }
    cliclack::outro("Configuration saved")?;
    Ok(())
}

fn configure_upstream(config_path: &Path) -> Result<()> {
    let host = input("Upstream SSH host", "github.com")?;
    let user = input("Upstream SSH user", "git")?;
    let port = input("Upstream SSH port", "22")?.parse::<u16>().context("parse SSH port")?;
    let identity = input_path("Upstream private key", "/etc/wt-git-proxy/upstream_ed25519")?;
    let known_hosts = input_path("Pinned known_hosts", "/etc/wt-git-proxy/upstream_known_hosts")?;
    for path in [&identity, &known_hosts] {
        if !path.is_absolute() || path.to_string_lossy().contains(|c: char| c.is_whitespace() || c == '"') {
            bail!("upstream credential paths must be absolute and contain no whitespace");
        }
    }
    let text = format!("Host {}\n  HostName {host}\n  User {user}\n  Port {port}\n  IdentityFile {}\n  IdentitiesOnly yes\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  BatchMode yes\n  PasswordAuthentication no\n", crate::config::UPSTREAM_ALIAS, identity.display(), known_hosts.display());
    crate::config::atomic_write(&crate::config::upstream_config_path(config_path), text.as_bytes(), 0o600)
}

fn set_policy(path: &Path, config: &mut ProxyConfig) -> Result<()> {
    let old = config.clone();
    config.write_prefix = input("Required write prefix", &config.write_prefix)?;
    config.allowed_branches = branch_list()?;
    if let Err(error) = config.save(path) { *config = old; return Err(error); }
    Ok(())
}

fn add_client(config_path: &Path) -> Result<()> {
    let executable = std::env::current_exe().context("find executable")?;
    let label = input("Client label", "agent")?;
    if cliclack::confirm("Generate a new Ed25519 client key?").initial_value(true).interact()? {
        let client = ClientConfig {
            host: input("Client-facing SSH host", "git-proxy.example.com")?,
            port: input("Client-facing SSH port", "22")?.parse().context("parse SSH port")?,
            user: "git-proxy".to_owned(),
            host_key_file: input_path("Public SSH host key", "/etc/ssh/ssh_host_ed25519_key.pub")?,
        };
        let output = input_path("Write client bundle to", "./wt-git-client")?;
        let (key, bundle) = add_generated_key(config_path, &executable, &client, &label, &output)?;
        cliclack::note("Client authorized", format!("{}\nBundle: {}", key.fingerprint, bundle.directory.display()))?;
    } else {
        let key = add_public_key(config_path, &executable, &label, &input("Client public key", "")?)?;
        cliclack::note("Client authorized", key.fingerprint)?;
    }
    Ok(())
}

fn remove_client(config_path: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    let keys = list_keys(config_path)?;
    if keys.is_empty() { bail!("no client keys are authorized"); }
    let mut selected = cliclack::select("Client to revoke");
    for key in keys { selected = selected.item(key.fingerprint.clone(), format!("{} ({})", key.label, key.fingerprint), ""); }
    remove_key(config_path, &executable, &selected.interact()?)
}

fn branch_list() -> Result<Vec<String>> {
    Ok(input("Exact allowed branches, comma-separated (optional)", "")?.split(',').map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned).collect())
}
fn input(prompt: &str, default: &str) -> Result<String> {
    let mut value = cliclack::input(prompt);
    if !default.is_empty() { value = value.default_input(default); }
    Ok(value.interact()?)
}
fn input_path(prompt: &str, default: &str) -> Result<PathBuf> { Ok(input(prompt, default)?.into()) }
