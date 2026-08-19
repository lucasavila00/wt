use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    pub version: u32,
    pub authorized_keys_file: PathBuf,
    pub executable: PathBuf,
    pub client: ClientConfig,
    pub write_prefix: String,
    #[serde(default)]
    pub allowed_branches: Vec<String>,
    #[serde(default)]
    pub upstreams: Vec<UpstreamConfig>,
    #[serde(default)]
    pub repositories: Vec<RepositoryConfig>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientConfig {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    pub host_key_file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: Option<u16>,
    pub private_key_file: PathBuf,
    pub known_hosts_file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryConfig {
    pub path: String,
    pub upstream: String,
    pub upstream_path: String,
}

fn default_ssh_port() -> u16 {
    22
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
        if self.version != CONFIG_VERSION {
            bail!(
                "unsupported proxy config version {}; expected {CONFIG_VERSION}",
                self.version
            );
        }
        for (name, path) in [
            ("authorized_keys_file", &self.authorized_keys_file),
            ("executable", &self.executable),
            ("client.host_key_file", &self.client.host_key_file),
        ] {
            require_absolute(name, path)?;
        }
        if !valid_host(&self.client.host) || self.client.port == 0 || !valid_atom(&self.client.user)
        {
            bail!("invalid client SSH endpoint");
        }
        wt_git_core::WritePolicy::new(self.write_prefix.clone(), self.allowed_branches.clone())?;

        let mut upstream_names = BTreeSet::new();
        for upstream in &self.upstreams {
            if !valid_atom(&upstream.name)
                || !upstream_names.insert(upstream.name.as_str())
                || !valid_host(&upstream.host)
                || !valid_atom(&upstream.user)
                || upstream.port == Some(0)
            {
                bail!("invalid or duplicate upstream `{}`", upstream.name);
            }
            require_absolute("upstream.private_key_file", &upstream.private_key_file)?;
            require_absolute("upstream.known_hosts_file", &upstream.known_hosts_file)?;
        }

        let mut public_paths = BTreeSet::new();
        for repository in &self.repositories {
            if !valid_repository_path(&repository.path, true)
                || !valid_repository_path(&repository.upstream_path, false)
                || !public_paths.insert(repository.path.as_str())
                || !upstream_names.contains(repository.upstream.as_str())
            {
                bail!("invalid or duplicate repository `{}`", repository.path);
            }
        }
        Ok(())
    }

    pub(crate) fn resolve_command(
        &self,
        command: &str,
    ) -> Result<(wt_git_core::GitService, &RepositoryConfig, &UpstreamConfig)> {
        for service in [
            wt_git_core::GitService::UploadPack,
            wt_git_core::GitService::ReceivePack,
        ] {
            for repository in &self.repositories {
                let expected = format!("{} '{}'", service.command(), repository.path);
                if command == expected {
                    let upstream = self
                        .upstreams
                        .iter()
                        .find(|upstream| upstream.name == repository.upstream)
                        .context("repository has no configured upstream")?;
                    return Ok((service, repository, upstream));
                }
            }
        }
        bail!("only configured Git repositories and services are allowed")
    }
}

fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn valid_atom(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_repository_path(value: &str, public: bool) -> bool {
    let value = if public {
        let Some(value) = value.strip_prefix('/') else {
            return false;
        };
        value
    } else {
        value.strip_prefix('/').unwrap_or(value)
    };
    value.ends_with(".git")
        && value
            .split('/')
            .all(|part| valid_atom(part) && part != "." && part != "..")
}

fn require_absolute(name: &str, path: &Path) -> Result<()> {
    if !path.is_absolute() {
        bail!("{name} must be an absolute path");
    }
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path.parent().context("managed file has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .with_context(|| format!("create {}", parent.display()))?;
    let temporary = path.with_extension("wt-new");
    if temporary.exists() {
        bail!("stale managed file exists: {}", temporary.display());
    }
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
    use std::os::unix::fs::PermissionsExt;

    pub(crate) fn config(root: &Path) -> ProxyConfig {
        ProxyConfig {
            version: CONFIG_VERSION,
            authorized_keys_file: root.join("authorized_keys"),
            executable: PathBuf::from("/usr/local/bin/wt-git-proxy"),
            client: ClientConfig {
                host: "proxy.example.test".to_owned(),
                port: 2222,
                user: "git-proxy".to_owned(),
                host_key_file: root.join("ssh_host_ed25519_key.pub"),
            },
            write_prefix: "refs/heads/tasks/".to_owned(),
            allowed_branches: vec!["refs/heads/main".to_owned()],
            upstreams: vec![UpstreamConfig {
                name: "origin".to_owned(),
                host: "git.example.test".to_owned(),
                user: "git".to_owned(),
                port: None,
                private_key_file: root.join("upstream-key"),
                known_hosts_file: root.join("upstream-known-hosts"),
            }],
            repositories: vec![RepositoryConfig {
                path: "/team/project.git".to_owned(),
                upstream: "origin".to_owned(),
                upstream_path: "team/project.git".to_owned(),
            }],
        }
    }

    #[test]
    fn config_is_strict_and_resolves_only_exact_commands() {
        let temp = tempfile::tempdir().unwrap();
        let config = config(temp.path());
        config.validate().unwrap();
        assert_eq!(
            config
                .resolve_command("git-upload-pack '/team/project.git'")
                .unwrap()
                .0,
            wt_git_core::GitService::UploadPack
        );
        assert!(config
            .resolve_command("git-upload-pack '/team/other.git'")
            .is_err());
        assert!(config.resolve_command("sh -c 'touch /tmp/owned'").is_err());
    }

    #[test]
    fn atomic_write_does_not_change_an_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();

        atomic_write(&temp.path().join("managed"), b"value", 0o600).unwrap();

        assert_eq!(
            fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}
