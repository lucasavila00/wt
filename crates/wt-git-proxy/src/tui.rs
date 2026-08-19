use crate::{
    add_generated_key, add_public_key, list_keys, remove_key, ClientConfig, ProxyConfig,
    RepositoryConfig, UpstreamConfig,
};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Eq, PartialEq)]
enum Action {
    AddUpstream,
    AddRepository,
    SetPolicy,
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
        initial_config(config_path)?
    };

    loop {
        let action = cliclack::select("What do you want to do?")
            .item(Action::AddUpstream, "Add upstream", "")
            .item(Action::AddRepository, "Add repository", "")
            .item(Action::SetPolicy, "Set write policy", "")
            .item(Action::AddClient, "Authorize client", "")
            .item(Action::RemoveClient, "Revoke client", "")
            .item(Action::Exit, "Exit", "")
            .interact()?;
        match action {
            Action::AddUpstream => add_upstream(config_path, &mut config)?,
            Action::AddRepository => add_repository(config_path, &mut config)?,
            Action::SetPolicy => set_policy(config_path, &mut config)?,
            Action::AddClient => add_client(config_path, &config)?,
            Action::RemoveClient => remove_client(config_path, &config)?,
            Action::Exit => break,
        }
    }
    cliclack::outro("Configuration saved")?;
    Ok(())
}

fn initial_config(path: &Path) -> Result<ProxyConfig> {
    let executable = std::env::current_exe().context("find wt-git-proxy executable")?;
    let authorized_keys_file = input_path(
        "Managed authorized_keys file",
        "/var/lib/wt-git-proxy/authorized_keys",
    )?;
    let host = input("Client-facing SSH host", "git-proxy.example.com")?;
    let port: u16 = cliclack::input("Client-facing SSH port")
        .default_input("22")
        .validate(|value: &String| match value.parse::<u16>() {
            Ok(0) | Err(_) => Err("Enter a port from 1 to 65535"),
            Ok(_) => Ok(()),
        })
        .interact::<String>()?
        .parse()
        .context("parse client SSH port")?;
    let user = input("Dedicated SSH user", "git-proxy")?;
    let host_key_file = input_path(
        "Public SSH host key file",
        "/etc/ssh/ssh_host_ed25519_key.pub",
    )?;
    let write_prefix = input("Required write prefix", "refs/heads/agents/")?;
    let allowed_branches = branch_list()?;
    let config = ProxyConfig {
        version: crate::config::CONFIG_VERSION,
        authorized_keys_file,
        executable,
        client: ClientConfig {
            host,
            port,
            user,
            host_key_file,
        },
        write_prefix,
        allowed_branches,
        upstreams: Vec::new(),
        repositories: Vec::new(),
    };
    config.save(path)?;
    Ok(config)
}

fn add_upstream(path: &Path, config: &mut ProxyConfig) -> Result<()> {
    let name = input("Upstream name", "origin")?;
    let host = input("Upstream SSH host", "github.com")?;
    let user = input("Upstream SSH user", "git")?;
    let port_text = input("Upstream SSH port (empty means 22)", "")?;
    let port = (!port_text.is_empty())
        .then(|| port_text.parse::<u16>().context("parse upstream SSH port"))
        .transpose()?;
    let private_key_file = input_path("Upstream private key file", "/etc/wt-git-proxy/id_ed25519")?;
    let known_hosts_file = input_path(
        "Pinned upstream known_hosts file",
        "/etc/wt-git-proxy/known_hosts",
    )?;
    config.upstreams.push(UpstreamConfig {
        name,
        host,
        user,
        port,
        private_key_file,
        known_hosts_file,
    });
    if let Err(error) = config.save(path) {
        config.upstreams.pop();
        return Err(error);
    }
    Ok(())
}

fn add_repository(path: &Path, config: &mut ProxyConfig) -> Result<()> {
    if config.upstreams.is_empty() {
        bail!("add an upstream before adding a repository");
    }
    let public = input("Public repository path", "/team/project.git")?;
    let mut upstream = cliclack::select("Upstream");
    for value in &config.upstreams {
        upstream = upstream.item(value.name.clone(), value.name.clone(), "");
    }
    let upstream = upstream.interact()?;
    let upstream_path = input("Path on the upstream", "team/project.git")?;
    config.repositories.push(RepositoryConfig {
        path: public,
        upstream,
        upstream_path,
    });
    if let Err(error) = config.save(path) {
        config.repositories.pop();
        return Err(error);
    }
    Ok(())
}

fn set_policy(path: &Path, config: &mut ProxyConfig) -> Result<()> {
    let old_prefix = config.write_prefix.clone();
    let old_branches = config.allowed_branches.clone();
    config.write_prefix = input("Required write prefix", &config.write_prefix)?;
    config.allowed_branches = branch_list()?;
    if let Err(error) = config.save(path) {
        config.write_prefix = old_prefix;
        config.allowed_branches = old_branches;
        return Err(error);
    }
    Ok(())
}

fn add_client(config_path: &Path, config: &ProxyConfig) -> Result<()> {
    let label = input("Client label", "agent")?;
    let generated = cliclack::confirm("Generate a new Ed25519 client key?")
        .initial_value(true)
        .interact()?;
    if generated {
        let output = input_path("Write the one-time client bundle to", "./wt-git-client")?;
        let (key, bundle) = add_generated_key(config_path, config, &label, &output)?;
        cliclack::note(
            "Client authorized",
            format!(
                "{}\nBundle: {}\nSSH alias: {}",
                key.fingerprint,
                bundle.directory.display(),
                bundle.alias
            ),
        )?;
    } else {
        let key = input("Client public key", "")?;
        let key = add_public_key(config_path, config, &label, &key)?;
        cliclack::note("Client authorized", key.fingerprint)?;
    }
    Ok(())
}

fn remove_client(config_path: &Path, config: &ProxyConfig) -> Result<()> {
    let keys = list_keys(config)?;
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
    remove_key(config_path, config, &selected.interact()?)
}

fn branch_list() -> Result<Vec<String>> {
    let value = input("Exact allowed branches, comma-separated (optional)", "")?;
    Ok(value
        .split(',')
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(str::to_owned)
        .collect())
}

fn input(prompt: &str, default: &str) -> Result<String> {
    let mut input = cliclack::input(prompt);
    if !default.is_empty() {
        input = input.default_input(default);
    }
    Ok(input.interact()?)
}

fn input_path(prompt: &str, default: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(input(prompt, default)?))
}
