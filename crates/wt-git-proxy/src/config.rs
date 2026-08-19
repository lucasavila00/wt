use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    pub write_prefix: String,
    #[serde(default)]
    pub allowed_branches: Vec<String>,
    pub providers: Vec<ProviderConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientConfig {
    pub host: String,
    pub port: u16,
    pub host_key_file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub host: String,
    pub user: String,
    pub port: u16,
    pub private_key_file: PathBuf,
    pub known_hosts_file: PathBuf,
}

impl ProxyConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read proxy config {}", path.display()))?;
        let config: Self = toml::from_str(&text)
            .with_context(|| format!("parse proxy config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = toml::to_string_pretty(self).context("encode proxy config")?;
        atomic_write(path, text.as_bytes(), 0o600)
    }

    pub fn validate(&self) -> Result<()> {
        self.policy()?;
        let mut hosts = BTreeSet::new();
        if self.providers.is_empty()
            || self.providers.iter().any(|provider| {
                !valid_ssh_name(&provider.host)
                    || !hosts.insert(provider.host.as_str())
                    || !valid_ssh_name(&provider.user)
                    || provider.port == 0
                    || !provider.private_key_file.is_absolute()
                    || !provider.known_hosts_file.is_absolute()
            })
        {
            bail!("invalid or duplicate Git provider");
        }
        Ok(())
    }

    pub(crate) fn policy(&self) -> Result<wt_git_core::WritePolicy> {
        wt_git_core::WritePolicy::new(
            format!("refs/heads/{}", self.write_prefix),
            self.allowed_branches
                .iter()
                .map(|branch| format!("refs/heads/{branch}")),
        )
    }

    pub(crate) fn resolve_command(
        &self,
        command: &str,
    ) -> Result<(wt_git_core::GitService, &ProviderConfig, String)> {
        for service in [
            wt_git_core::GitService::UploadPack,
            wt_git_core::GitService::ReceivePack,
        ] {
            let prefix = format!("{} '", service.command());
            if let Some(path) = command
                .strip_prefix(&prefix)
                .and_then(|value| value.strip_suffix('\''))
            {
                let path = path.strip_prefix('/').unwrap_or(path);
                if let Some((host, repository)) = path.split_once('/') {
                    if valid_repository_path(repository) {
                        let provider = self
                            .providers
                            .iter()
                            .find(|provider| provider.host == host)
                            .with_context(|| format!("Git provider `{host}` is not configured"))?;
                        return Ok((service, provider, repository.to_owned()));
                    }
                }
            }
        }
        bail!("only safe Git repository paths and services are allowed")
    }
}

pub(crate) fn authorized_keys_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name("authorized_keys")
}

fn valid_repository_path(value: &str) -> bool {
    !value.is_empty()
        && value.ends_with(".git")
        && value
            .strip_prefix('/')
            .unwrap_or(value)
            .split('/')
            .all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
            })
}

fn valid_ssh_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path.parent().context("managed file has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .with_context(|| format!("create {}", parent.display()))?;
    let temporary = path.with_extension("wt-new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", temporary.display()))?;
    drop(file);
    fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_is_only_the_write_policy() {
        let config = ProxyConfig {
            write_prefix: "tasks/".to_owned(),
            allowed_branches: vec!["main".to_owned()],
            providers: vec![ProviderConfig {
                host: "github.com".to_owned(),
                user: "git".to_owned(),
                port: 22,
                private_key_file: "/etc/wt-git-proxy/github_ed25519".into(),
                known_hosts_file: "/etc/wt-git-proxy/github_known_hosts".into(),
            }],
        };
        config.validate().unwrap();
        assert_eq!(
            config
                .resolve_command("git-upload-pack 'github.com/team/project.git'")
                .unwrap(),
            (
                wt_git_core::GitService::UploadPack,
                &config.providers[0],
                "team/project.git".to_owned()
            )
        );
        assert!(config
            .resolve_command("git-upload-pack '../secret.git'")
            .is_err());
        assert!(toml::from_str::<ProxyConfig>("write_prefix='x/'\nextra=1").is_err());
    }
}
