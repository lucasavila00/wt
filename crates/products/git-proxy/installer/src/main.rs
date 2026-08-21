use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use wt_git_proxy::{ProviderConfig, ProxyConfig};
use wt_installer_support::cmd;
use wt_installer_support::{
    expand_home, prepare_ssh_credentials, sudo_install_owned, temporary_credential,
    validate_ssh_files, Runner, SshCredentialInput, SystemRunner, TerminalPassphrasePrompt,
};

const CONFIG_DIRECTORY: &str = "/etc/wt-git-proxy";
const CONFIG_PATH: &str = "/etc/wt-git-proxy/config.toml";
const PROXY_USER: &str = "git-proxy";
const PROXY_GROUP: &str = "git-proxy";
const PUBLIC_IP_URL: &str = "https://api.ipify.org";

#[derive(Debug, Parser)]
#[command(name = "wt-git-proxy-installer")]
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
    #[serde(default = "default_client_port")]
    client_port: u16,
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
        self.runtime_config("127.0.0.1").validate()
    }

    fn runtime_config(&self, client_host: &str) -> ProxyConfig {
        ProxyConfig {
            client_host: client_host.to_owned(),
            client_port: self.client_port,
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

fn default_client_port() -> u16 {
    22
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
    let client_host = prompt_client_host(runner, input.client_port)?;
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

    let runtime = toml::to_string_pretty(&input.runtime_config(&client_host))
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

fn prompt_client_host(runner: &impl Runner, port: u16) -> Result<String> {
    let public_ip = match current_public_ipv4(runner) {
        Ok(address) => Some(address),
        Err(error) => {
            cliclack::note(
                "Public IP lookup failed",
                format!("Enter the agent-facing address manually.\n{error:#}"),
            )?;
            None
        }
    };
    let suggestion = public_ip.map(|address| format!("{PROXY_USER}@{address}"));
    let mut prompt = cliclack::input("Agent SSH destination (user@host)");
    if let Some(suggestion) = &suggestion {
        prompt = prompt.default_input(suggestion);
    }
    let destination: String = prompt
        .validate(|value: &String| validate_client_destination(value))
        .interact()
        .context("read agent SSH destination")?;
    let host = parse_client_destination(&destination)
        .map_err(anyhow::Error::msg)?
        .to_owned();
    cliclack::note(
        "Agent connection",
        format!("{PROXY_USER}@{host} on port {port}\nThe port comes from the install config."),
    )?;
    Ok(host)
}

fn current_public_ipv4(runner: &impl Runner) -> Result<Ipv4Addr> {
    let output = runner.output(cmd!(
        "curl",
        "--fail",
        "--silent",
        "--show-error",
        "--max-time",
        "5",
        PUBLIC_IP_URL,
    ))?;
    if !output.status.success() {
        anyhow::bail!(
            "look up the server public IP: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let body = String::from_utf8(output.stdout).context("decode the public IP response")?;
    parse_public_ipv4(&body)
}

fn parse_public_ipv4(value: &str) -> Result<Ipv4Addr> {
    value
        .trim()
        .parse()
        .context("public IP service returned an invalid IPv4 address")
}

fn validate_client_destination(value: &str) -> std::result::Result<(), &'static str> {
    parse_client_destination(value).map(|_| ())
}

fn parse_client_destination(value: &str) -> std::result::Result<&str, &'static str> {
    let (user, host) = value
        .split_once('@')
        .ok_or("Use the form git-proxy@203.0.113.10")?;
    if user != PROXY_USER {
        return Err("The SSH user must be git-proxy");
    }
    if host.is_empty()
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("Enter a valid IP address or DNS name after git-proxy@");
    }
    Ok(host)
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

        insta::assert_snapshot!(toml::to_string_pretty(&input.runtime_config("203.0.113.10")).unwrap(), @r###"
        client_host = "203.0.113.10"
        client_port = 22
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
        assert!(input
            .runtime_config("203.0.113.10")
            .allowed_branches
            .is_empty());
    }

    #[test]
    fn client_destination_is_normal_ssh_user_at_host() {
        assert_eq!(
            parse_client_destination("git-proxy@203.0.113.10"),
            Ok("203.0.113.10")
        );
        assert_eq!(
            parse_client_destination("git-proxy@proxy.example.com"),
            Ok("proxy.example.com")
        );
        assert_eq!(
            parse_client_destination("wt@203.0.113.10"),
            Err("The SSH user must be git-proxy")
        );
    }

    #[test]
    fn client_port_is_file_only_and_defaults_to_ssh() {
        let default: InstallInput =
            toml::from_str("version=1\nwrite_prefix='agents/'\nproviders=[]\n").unwrap();
        let custom: InstallInput =
            toml::from_str("version=1\nclient_port=2222\nwrite_prefix='agents/'\nproviders=[]\n")
                .unwrap();

        assert_eq!(default.client_port, 22);
        assert_eq!(custom.client_port, 2222);
    }

    #[test]
    fn public_ip_response_is_strict_ipv4() {
        assert_eq!(
            parse_public_ipv4("203.0.113.10\n").unwrap(),
            Ipv4Addr::new(203, 0, 113, 10)
        );
        assert!(parse_public_ipv4("proxy.example.com").is_err());
        assert!(parse_public_ipv4("2001:db8::1").is_err());
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
