use crate::{
    add_generated_key, add_public_key, list_keys, remove_key, ProviderConfig, ProxyConfig,
};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Eq, PartialEq)]
enum Action {
    Provider,
    Policy,
    AddClient,
    RemoveClient,
    Exit,
}

pub fn run_tui(config_path: &Path) -> Result<()> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        bail!("wt-git-proxy tui requires an interactive terminal");
    }
    cliclack::intro("Configure WT Git proxy")?;
    let mut config = if config_path.exists() {
        ProxyConfig::load(config_path)?
    } else {
        let config = ProxyConfig {
            write_prefix: input("Required write prefix", "agents/")?,
            allowed_branches: branch_list()?,
            providers: vec![provider_config()?],
        };
        config.save(config_path)?;
        config
    };
    loop {
        match cliclack::select("What do you want to do?")
            .item(Action::Provider, "Add or update provider", "")
            .item(Action::Policy, "Set write policy", "")
            .item(Action::AddClient, "Authorize client", "")
            .item(Action::RemoveClient, "Revoke client", "")
            .item(Action::Exit, "Exit", "")
            .interact()?
        {
            Action::Provider => configure_provider(config_path, &mut config)?,
            Action::Policy => set_policy(config_path, &mut config)?,
            Action::AddClient => add_client(config_path)?,
            Action::RemoveClient => remove_client(config_path)?,
            Action::Exit => break,
        }
    }
    cliclack::outro("Configuration saved")?;
    Ok(())
}

fn provider_config() -> Result<ProviderConfig> {
    let host = input("Provider SSH host", "github.com")?;
    let user = input("Provider SSH user", "git")?;
    let port = input("Provider SSH port", "22")?
        .parse::<u16>()
        .context("parse SSH port")?;
    let identity = input_path("Provider private key", "/etc/wt-git-proxy/provider_ed25519")?;
    let known_hosts = input_path(
        "Pinned known_hosts",
        "/etc/wt-git-proxy/provider_known_hosts",
    )?;
    Ok(ProviderConfig {
        host,
        user,
        port,
        private_key_file: identity,
        known_hosts_file: known_hosts,
    })
}

fn configure_provider(path: &Path, config: &mut ProxyConfig) -> Result<()> {
    let provider = provider_config()?;
    config
        .providers
        .retain(|existing| existing.host != provider.host);
    config.providers.push(provider);
    config.save(path)
}

fn set_policy(path: &Path, config: &mut ProxyConfig) -> Result<()> {
    config.write_prefix = input("Required write prefix", &config.write_prefix)?;
    config.allowed_branches = branch_list()?;
    config.save(path)
}

fn add_client(config_path: &Path) -> Result<()> {
    let executable = std::env::current_exe().context("find executable")?;
    let label = input("Client label", "agent")?;
    if cliclack::confirm("Generate a new Ed25519 client key?")
        .initial_value(true)
        .interact()?
    {
        let output = input_path("Write client bundle to", "./wt-git-client")?;
        let key = add_generated_key(config_path, &executable, &label, &output)?;
        cliclack::note(
            "Client authorized",
            format!("{}\nBundle: {}", key.fingerprint, output.display()),
        )?;
    } else {
        let key = add_public_key(
            config_path,
            &executable,
            &label,
            &input("Client public key", "")?,
        )?;
        cliclack::note("Client authorized", key.fingerprint)?;
    }
    Ok(())
}

fn remove_client(config_path: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    let keys = list_keys(config_path)?;
    if keys.is_empty() {
        bail!("no client keys are authorized");
    }
    let mut selected = cliclack::select("Client to revoke");
    for key in keys {
        selected = selected.item(
            key.fingerprint.clone(),
            format!("{} ({})", key.label, key.fingerprint),
            "",
        );
    }
    remove_key(config_path, &executable, &selected.interact()?)
}

fn branch_list() -> Result<Vec<String>> {
    Ok(
        input("Exact allowed branches, comma-separated (optional)", "")?
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}
fn input(prompt: &str, default: &str) -> Result<String> {
    let mut value = cliclack::input(prompt);
    if !default.is_empty() {
        value = value.default_input(default);
    }
    Ok(value.interact()?)
}
fn input_path(prompt: &str, default: &str) -> Result<PathBuf> {
    Ok(input(prompt, default)?.into())
}
