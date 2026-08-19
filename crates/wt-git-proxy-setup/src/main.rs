use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use wt_command::cmd;
use wt_git_proxy::{ProviderConfig, ProxyConfig};
use wt_setup_core::{
    expand_home, prepare_ssh_credentials, sudo_install_owned, temporary_credential,
    validate_ssh_files, Runner, SshCredentialInput, SystemRunner, TerminalPassphrasePrompt,
};

const CONFIG_DIRECTORY: &str = "/etc/wt-git-proxy";
const CONFIG_PATH: &str = "/etc/wt-git-proxy/config.toml";
const PROXY_USER: &str = "git-proxy";
const PROXY_GROUP: &str = "git-proxy";

#[derive(Debug, Parser)]
#[command(name = "wt-git-proxy-setup")]
struct Cli {
    #[command(subcommand)]
    command: SetupCommand,
}

#[derive(Debug, Subcommand)]
enum SetupCommand {
    Validate {
        #[arg(long)]
        config: PathBuf,
    },
    Install {
        #[arg(long)]
        config: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallInput {
    version: u32,
    write_prefix: String,
    #[serde(default)]
    allowed_branches: Vec<String>,
    providers: Vec<InstallProvider>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallProvider {
    host: String,
    private_key_file: PathBuf,
    known_hosts_file: PathBuf,
}

impl InstallInput {
    fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read install input {}", path.display()))?;
        let mut input: Self = toml::from_str(&text)
            .with_context(|| format!("parse install input {}", path.display()))?;
        for provider in &mut input.providers {
            provider.private_key_file = expand_home(
                &provider.private_key_file,
                &format!("providers[{}].private_key_file", provider.host),
            )
            .map_err(anyhow::Error::msg)?;
            provider.known_hosts_file = expand_home(
                &provider.known_hosts_file,
                &format!("providers[{}].known_hosts_file", provider.host),
            )
            .map_err(anyhow::Error::msg)?;
        }
        input.validate()?;
        Ok(input)
    }

    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            anyhow::bail!(
                "unsupported install input version {}; expected 1",
                self.version
            );
        }
        self.runtime_config().validate()
    }

    fn runtime_config(&self) -> ProxyConfig {
        ProxyConfig {
            write_prefix: self.write_prefix.clone(),
            allowed_branches: self.allowed_branches.clone(),
            providers: self
                .providers
                .iter()
                .enumerate()
                .map(|(index, provider)| ProviderConfig {
                    host: provider.host.clone(),
                    user: "git".to_owned(),
                    port: 22,
                    private_key_file: provider_runtime_directory(index).join("id_ed25519"),
                    known_hosts_file: provider_runtime_directory(index).join("known_hosts"),
                })
                .collect(),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("\nWT Git proxy setup failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let runner = SystemRunner;
    match Cli::parse().command {
        SetupCommand::Validate { config } => {
            let input = InstallInput::load(&config)?;
            validate_files(&input)?;
            println!("Configuration is valid: {}", config.display());
        }
        SetupCommand::Install { config } => install(&runner, &config)?,
    }
    Ok(())
}

fn validate_files(input: &InstallInput) -> Result<()> {
    for provider in &input.providers {
        validate_ssh_files(&credential_input(provider))?;
    }
    Ok(())
}

fn install(runner: &impl Runner, path: &Path) -> Result<()> {
    let input = InstallInput::load(path)?;
    let prompt = TerminalPassphrasePrompt::new(ssh_key_passphrase_context);
    let mut credentials = Vec::with_capacity(input.providers.len());
    for provider in &input.providers {
        credentials.push(prepare_ssh_credentials(
            runner,
            &prompt,
            &credential_input(provider),
        )?);
    }

    runner.run(
        cmd!(
            "sudo",
            "install",
            "-d",
            "-o",
            PROXY_USER,
            "-g",
            PROXY_GROUP,
            "-m",
            "0700",
            CONFIG_DIRECTORY,
        ),
        "create Git proxy configuration directory",
    )?;
    for (index, credential) in credentials.into_iter().enumerate() {
        let directory = provider_runtime_directory(index);
        runner.run(
            cmd!(
                "sudo",
                "install",
                "-d",
                "-o",
                PROXY_USER,
                "-g",
                PROXY_GROUP,
                "-m",
                "0700",
                &directory,
            ),
            "create provider credential directory",
        )?;
        sudo_install_owned(
            runner,
            credential.private_key.path(),
            &directory.join("id_ed25519"),
            PROXY_USER,
            PROXY_GROUP,
            0o600,
        )?;
        sudo_install_owned(
            runner,
            credential.known_hosts.path(),
            &directory.join("known_hosts"),
            PROXY_USER,
            PROXY_GROUP,
            0o600,
        )?;
    }

    let runtime = toml::to_string_pretty(&input.runtime_config())
        .context("encode Git proxy runtime config")?;
    let temporary = temporary_credential(runtime.as_bytes())?;
    sudo_install_owned(
        runner,
        temporary.path(),
        Path::new(CONFIG_PATH),
        PROXY_USER,
        PROXY_GROUP,
        0o600,
    )?;
    println!("WT Git proxy configuration installed: {CONFIG_PATH}");
    Ok(())
}

fn credential_input(provider: &InstallProvider) -> SshCredentialInput<'_> {
    SshCredentialInput {
        name: &provider.host,
        host: &provider.host,
        private_key_file: &provider.private_key_file,
        public_key_file: None,
        known_hosts_file: &provider.known_hosts_file,
    }
}

fn provider_runtime_directory(index: usize) -> PathBuf {
    Path::new(CONFIG_DIRECTORY)
        .join("providers")
        .join(index.to_string())
}

fn ssh_key_passphrase_context(host: &str, path: &Path) -> String {
    format!(
        "Git provider SSH key is passphrase-protected\n\n\
Provider: {host}\n\
Key: {}\n\
The proxy needs an unlocked copy so it can fetch and push.\n\
The original key will not be changed. Setup installs the unlocked copy with\n\
mode 0600 for the git-proxy account and removes the temporary copy.",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_input_materializes_runtime_defaults() {
        let input: InstallInput = toml::from_str(
            r#"
version = 1
write_prefix = "agents/"
allowed_branches = ["main"]

[[providers]]
host = "github.com"
private_key_file = "/home/wt/.ssh/github_ed25519"
known_hosts_file = "/home/wt/.ssh/github_known_hosts"
"#,
        )
        .unwrap();
        input.validate().unwrap();

        insta::assert_snapshot!(toml::to_string_pretty(&input.runtime_config()).unwrap(), @r###"
        write_prefix = "agents/"
        allowed_branches = ["main"]

        [[providers]]
        host = "github.com"
        user = "git"
        port = 22
        private_key_file = "/etc/wt-git-proxy/providers/0/id_ed25519"
        known_hosts_file = "/etc/wt-git-proxy/providers/0/known_hosts"
        "###);
    }

    #[test]
    fn exact_branch_list_may_be_omitted() {
        let input: InstallInput = toml::from_str(
            r#"
version = 1
write_prefix = "agents/"

[[providers]]
host = "github.com"
private_key_file = "/home/wt/.ssh/github_ed25519"
known_hosts_file = "/home/wt/.ssh/github_known_hosts"
"#,
        )
        .unwrap();

        input.validate().unwrap();
        assert!(input.runtime_config().allowed_branches.is_empty());
    }

    #[test]
    fn password_explanation_is_direct() {
        insta::assert_snapshot!(
            ssh_key_passphrase_context("github.com", Path::new("/home/wt/.ssh/github")),
            @"
        Git provider SSH key is passphrase-protected

        Provider: github.com
        Key: /home/wt/.ssh/github
        The proxy needs an unlocked copy so it can fetch and push.
        The original key will not be changed. Setup installs the unlocked copy with
        mode 0600 for the git-proxy account and removes the temporary copy.
        "
        );
    }
}
