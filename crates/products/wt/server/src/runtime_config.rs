use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use wt_libvirt_kvm::{CodexMounts, MachineConfig};
use wt_retained_worlds::devcontainer::{BootstrapPolicy, PackageVersions, ProvisionerConfig};

pub const DEFAULT_AGENT_GIT_VSOCK_PORT: u32 = wt_agent_git_gateway::VSOCK_PORT;
pub const AGENT_GIT_VSOCK_PORT_ENV: &str = wt_agent_git_gateway::VSOCK_PORT_ENV;

pub const SERVER_CONFIG_PATH: &str = "/etc/wt/server.toml";
pub const CODEX_AUTH_PATH: &str = "/home/wt/.codex/auth.json";
pub const CODEX_AUTH_SHARE_DIR: &str = "/home/wt/.codex/.wt-auth";
pub const CODEX_SESSIONS_PATH: &str = "/home/wt/.codex/sessions";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub version: u32,
    pub image: ImageConfig,
    pub libvirt: ServerLibvirtConfig,
    pub registry_cache: RegistryCacheConfig,
    pub agent_git: AgentGitConfig,
    pub guest: GuestConfig,
    pub install: InstallConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCacheConfig {
    pub state_dir: PathBuf,
    pub port: u16,
    pub max_size_gib: u64,
    pub registries: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentGitConfig {
    #[serde(default = "default_agent_git_vsock_port")]
    pub vsock_port: u32,
    pub github: Option<AgentGitProviderConfig>,
    pub gitlab: Option<AgentGitProviderConfig>,
}

fn default_agent_git_vsock_port() -> u32 {
    DEFAULT_AGENT_GIT_VSOCK_PORT
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentGitProviderConfig {
    pub host: String,
}

/// Golden image path used by the server at runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageConfig {
    pub devcontainer_path: PathBuf,
    pub host_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerLibvirtConfig {
    pub network: String,
    pub worlds_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GuestConfig {
    pub boot_timeout_seconds: u64,
    pub recipe_timeout_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallConfig {
    pub binary_dir: PathBuf,
}

impl ServerConfig {
    pub fn load() -> Result<Self, String> {
        let config = Self::load_from(Path::new(SERVER_CONFIG_PATH))?;
        config.validate_codex_sources()?;
        Ok(config)
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("read config {}: {error}", path.display()))?;
        let mut config: Self = toml::from_str(&contents)
            .map_err(|error| format!("parse config {}: {error}", path.display()))?;
        if let Some(port) =
            wt_agent_git_gateway::vsock_port_from_env().map_err(|error| error.to_string())?
        {
            config.agent_git.vsock_port = port;
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported config version {}; expected 1",
                self.version
            ));
        }
        for (name, path) in [
            ("image.devcontainer_path", &self.image.devcontainer_path),
            ("image.host_path", &self.image.host_path),
            ("libvirt.worlds_dir", &self.libvirt.worlds_dir),
            ("registry_cache.state_dir", &self.registry_cache.state_dir),
            ("install.binary_dir", &self.install.binary_dir),
        ] {
            if !path.is_absolute() {
                return Err(format!("{name} must be an absolute path"));
            }
            if path == Path::new("/")
                || path.components().any(|component| {
                    !matches!(component, Component::RootDir | Component::Normal(_))
                })
            {
                return Err(format!(
                    "{name} must be an absolute normalized path below /"
                ));
            }
        }
        for (name, path) in [
            ("image.devcontainer_path", &self.image.devcontainer_path),
            ("image.host_path", &self.image.host_path),
        ] {
            if path.extension().and_then(|value| value.to_str()) != Some("qcow2") {
                return Err(format!("{name} must end in .qcow2"));
            }
        }
        let devcontainer_image_dir = self
            .image
            .devcontainer_path
            .parent()
            .ok_or_else(|| "image.devcontainer_path must have a parent directory".to_owned())?;
        let host_image_dir = self
            .image
            .host_path
            .parent()
            .ok_or_else(|| "image.host_path must have a parent directory".to_owned())?;
        if self.image.devcontainer_path == self.image.host_path {
            return Err("devcontainer and host images must use different files".to_owned());
        }
        if devcontainer_image_dir != host_image_dir {
            return Err("devcontainer and host images must use the same directory".to_owned());
        }
        for (left_name, left, right_name, right) in [
            (
                "image directory",
                devcontainer_image_dir,
                "libvirt.worlds_dir",
                self.libvirt.worlds_dir.as_path(),
            ),
            (
                "image directory",
                devcontainer_image_dir,
                "install.binary_dir",
                self.install.binary_dir.as_path(),
            ),
            (
                "libvirt.worlds_dir",
                self.libvirt.worlds_dir.as_path(),
                "install.binary_dir",
                self.install.binary_dir.as_path(),
            ),
            (
                "registry_cache.state_dir",
                self.registry_cache.state_dir.as_path(),
                "image directory",
                devcontainer_image_dir,
            ),
            (
                "registry_cache.state_dir",
                self.registry_cache.state_dir.as_path(),
                "libvirt.worlds_dir",
                self.libvirt.worlds_dir.as_path(),
            ),
            (
                "registry_cache.state_dir",
                self.registry_cache.state_dir.as_path(),
                "install.binary_dir",
                self.install.binary_dir.as_path(),
            ),
        ] {
            if left.starts_with(right) || right.starts_with(left) {
                return Err(format!("{left_name} and {right_name} must not overlap"));
            }
        }
        if self.libvirt.network.trim().is_empty() {
            return Err("libvirt.network must not be empty".to_owned());
        }
        self.validate_registry_cache()?;
        self.validate_agent_git()?;
        if self.guest.boot_timeout_seconds == 0 || self.guest.recipe_timeout_seconds == 0 {
            return Err("guest timeout values must be greater than zero".to_owned());
        }
        Ok(())
    }

    pub fn devcontainer_machine_config(&self) -> MachineConfig {
        MachineConfig {
            image: self.image.devcontainer_path.clone(),
            worlds_dir: self.libvirt.worlds_dir.clone(),
            network: self.libvirt.network.clone(),
            boot_timeout: Duration::from_secs(self.guest.boot_timeout_seconds),
            codex_mounts: Some(self.codex_mounts()),
        }
    }

    pub fn host_machine_config(&self) -> MachineConfig {
        MachineConfig {
            image: self.image.host_path.clone(),
            worlds_dir: self.libvirt.worlds_dir.clone(),
            network: self.libvirt.network.clone(),
            boot_timeout: Duration::from_secs(self.guest.boot_timeout_seconds),
            codex_mounts: Some(self.codex_mounts()),
        }
    }

    pub fn provisioner_config(
        &self,
        registry_cache_url: String,
        retained: wt_retained_worlds::RetainedConfig,
    ) -> Result<ProvisionerConfig, String> {
        let bootstrap = self.bootstrap_policy()?;
        Ok(ProvisionerConfig {
            app_pane_binary: self.install.binary_dir.join("wt-devcontainer-pane"),
            app_info_binary: self.install.binary_dir.join("wt-devcontainer-info"),
            app_proxy_binary: self.install.binary_dir.join("wt-devcontainer-ssh-proxy"),
            registry_cache_url,
            registry_cache_ca_file: self.registry_cache.state_dir.join("ca/ca.crt"),
            recipe_timeout: Duration::from_secs(self.guest.recipe_timeout_seconds),
            bootstrap,
            retained,
        })
    }

    pub fn retained_config(&self) -> wt_retained_worlds::RetainedConfig {
        wt_retained_worlds::RetainedConfig {
            agent_git: wt_retained_worlds::AgentGitConfig {
                relay_binary: self.install.binary_dir.join("wt-agent-git-gateway-relay"),
                remote_binary: self.install.binary_dir.join("git-remote-wt-agent"),
                cli_binary: self.install.binary_dir.join("wt-git-hosting"),
                provider_hosts: self.agent_git_provider_hosts(),
                vsock_port: self.agent_git.vsock_port,
            },
            wt_codex_integration_binary: self.install.binary_dir.join("wt-codex-integration"),
        }
    }

    pub fn validate_codex_sources(&self) -> Result<(), String> {
        self.validate_codex_login()?;
        let sessions = Path::new(CODEX_SESSIONS_PATH);
        match std::fs::symlink_metadata(sessions) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "Codex sessions path is not a directory: {}",
                    sessions.display()
                ))
            }
            Err(error) => {
                return Err(format!(
                    "inspect Codex sessions path {}: {error}",
                    sessions.display()
                ))
            }
        }
        let auth = Path::new(CODEX_AUTH_PATH);
        let metadata = std::fs::symlink_metadata(auth).map_err(|error| {
            format!(
                "inspect Codex authentication file {}: {error}",
                auth.display()
            )
        })?;
        let share = Path::new(CODEX_AUTH_SHARE_DIR);
        let shared_auth = share.join("auth.json");
        let shared_metadata = std::fs::symlink_metadata(&shared_auth).map_err(|error| {
            format!(
                "inspect Codex authentication share {}: {error}",
                shared_auth.display()
            )
        })?;
        if shared_metadata.file_type().is_symlink()
            || !shared_metadata.is_file()
            || metadata.dev() != shared_metadata.dev()
            || metadata.ino() != shared_metadata.ino()
        {
            return Err(format!(
                "Codex authentication share must be a hard link to {}: {}",
                auth.display(),
                shared_auth.display()
            ));
        }
        let entries = std::fs::read_dir(share)
            .map_err(|error| {
                format!(
                    "read Codex authentication share {}: {error}",
                    share.display()
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                format!(
                    "read Codex authentication share {}: {error}",
                    share.display()
                )
            })?;
        if entries.len() != 1 {
            return Err(format!(
                "Codex authentication share must contain only auth.json: {}",
                share.display()
            ));
        }
        Ok(())
    }

    pub fn validate_codex_login(&self) -> Result<(), String> {
        let auth = Path::new(CODEX_AUTH_PATH);
        let metadata = std::fs::symlink_metadata(auth).map_err(|error| {
            format!(
                "inspect Codex authentication file {}: {error}",
                auth.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Codex authentication path must be a regular, non-symlink file: {}",
                auth.display()
            ));
        }
        Ok(())
    }

    fn codex_mounts(&self) -> CodexMounts {
        CodexMounts {
            sessions: PathBuf::from(CODEX_SESSIONS_PATH),
            auth: PathBuf::from(CODEX_AUTH_SHARE_DIR),
        }
    }

    fn agent_git_provider_hosts(&self) -> Vec<String> {
        [
            self.agent_git.github.as_ref(),
            self.agent_git.gitlab.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|provider| provider.host.clone())
        .collect()
    }

    fn bootstrap_policy(&self) -> Result<BootstrapPolicy, String> {
        #[derive(Deserialize)]
        struct RawManifest {
            packages: PackageVersions,
            devcontainer_cli: String,
        }
        let manifest_path = PathBuf::from(format!(
            "{}.manifest.json",
            self.image.devcontainer_path.display()
        ));
        let bytes = std::fs::read(&manifest_path)
            .map_err(|error| format!("read image manifest {}: {error}", manifest_path.display()))?;
        let manifest: RawManifest = serde_json::from_slice(&bytes).map_err(|error| {
            format!("parse image manifest {}: {error}", manifest_path.display())
        })?;
        BootstrapPolicy::from_installed_packages(
            manifest.packages,
            manifest.devcontainer_cli,
            wt_libvirt_kvm::MACHINE_BOOTSTRAP_PACKAGES,
        )
    }

    fn validate_registry_cache(&self) -> Result<(), String> {
        if self.registry_cache.port == 0 || self.registry_cache.max_size_gib == 0 {
            return Err("registry cache port and size must be greater than zero".to_owned());
        }
        if self.registry_cache.registries.is_empty() {
            return Err("registry_cache.registries must not be empty".to_owned());
        }
        let mut registries = std::collections::BTreeSet::new();
        for registry in &self.registry_cache.registries {
            if !valid_registry_host(registry) || !registries.insert(registry.as_str()) {
                return Err(format!(
                    "invalid or duplicate registry cache host: {registry}"
                ));
            }
        }
        Ok(())
    }

    fn validate_agent_git(&self) -> Result<(), String> {
        if self.agent_git.vsock_port == 0 || self.agent_git.vsock_port == u32::MAX {
            return Err("agent_git.vsock_port must be a concrete nonzero port".to_owned());
        }
        let providers = [
            ("agent_git.github.host", self.agent_git.github.as_ref()),
            ("agent_git.gitlab.host", self.agent_git.gitlab.as_ref()),
        ];
        if providers.iter().all(|(_, provider)| provider.is_none()) {
            return Err("at least one agent Git provider is required".to_owned());
        }
        let mut hosts = std::collections::BTreeSet::new();
        for (name, provider) in providers {
            let Some(provider) = provider else { continue };
            if !valid_git_host(&provider.host) || !hosts.insert(provider.host.as_str()) {
                return Err(format!("invalid or duplicate {name}: {}", provider.host));
            }
        }
        Ok(())
    }
}

fn valid_git_host(value: &str) -> bool {
    !value.is_empty()
        && value == value.to_ascii_lowercase()
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn valid_registry_host(value: &str) -> bool {
    !value.is_empty()
        && value == value.to_ascii_lowercase()
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b':')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
version = 1

[image]
devcontainer_path = "/var/lib/wt/images/devcontainer.qcow2"
host_path = "/var/lib/wt/images/host.qcow2"

[libvirt]
network = "default"
worlds_dir = "/var/lib/libvirt/images/wt"

[registry_cache]
state_dir = "/var/lib/wt/registry-cache"
port = 3128
max_size_gib = 64
registries = ["docker.io", "mcr.microsoft.com"]

[agent_git.github]
host = "github.com"

[guest]
boot_timeout_seconds = 300
recipe_timeout_seconds = 900

[install]
binary_dir = "/usr/local/bin"
"#;

    fn parse(value: &str) -> Result<(ServerConfig, MachineConfig), String> {
        let config: ServerConfig = toml::from_str(value).map_err(|error| error.to_string())?;
        config.validate()?;
        let machine = config.devcontainer_machine_config();
        Ok((config, machine))
    }

    #[test]
    fn complete_config_is_valid() {
        let (config, machine) = parse(VALID).unwrap();
        assert_eq!(config.agent_git.vsock_port, DEFAULT_AGENT_GIT_VSOCK_PORT);
        assert_eq!(
            machine.image,
            Path::new("/var/lib/wt/images/devcontainer.qcow2")
        );
        assert_eq!(machine.network, "default");
        assert_eq!(
            machine.codex_mounts,
            Some(CodexMounts {
                sessions: PathBuf::from(CODEX_SESSIONS_PATH),
                auth: PathBuf::from(CODEX_AUTH_SHARE_DIR),
            })
        );
    }

    #[test]
    fn missing_and_unknown_fields_fail() {
        assert!(parse(&VALID.replace("recipe_timeout_seconds = 900\n", "")).is_err());
        assert!(parse(&VALID.replace(
            "recipe_timeout_seconds = 900",
            "recipe_timeout_seconds = 900\nfallback = true"
        ))
        .is_err());
        assert!(parse(&VALID.replace(
            "registries = [\"docker.io\", \"mcr.microsoft.com\"]",
            "registries = [\"docker.io\", \"mcr.microsoft.com\"]\npreload_images = [\"redis:7-alpine\"]"
        ))
        .is_err());
    }

    #[test]
    fn invalid_values_fail() {
        assert!(parse(&VALID.replace("/usr/local/bin", "relative/bin")).is_err());
        assert!(parse(&VALID.replace("/usr/local/bin", "/")).is_err());
        assert!(parse(&VALID.replace("/usr/local/bin", "/usr/../bin")).is_err());
        assert!(parse(&VALID.replace("/usr/local/bin", "/var/lib/wt")).is_err());
        assert!(parse(
            &VALID.replace("recipe_timeout_seconds = 900", "recipe_timeout_seconds = 0")
        )
        .is_err());
        assert!(parse(&VALID.replace("max_size_gib = 64", "max_size_gib = 0")).is_err());
        assert!(parse(&VALID.replace(
            "[agent_git.github]",
            "[agent_git]\nvsock_port = 0\n\n[agent_git.github]"
        ))
        .is_err());
        assert!(parse(&VALID.replace(
            "host_path = \"/var/lib/wt/images/host.qcow2\"",
            "host_path = \"/var/lib/wt/images/host.qcow2\"\nsource_url = \"https://example.com/img\""
        ))
        .is_err());
        assert_eq!(
            parse(&VALID.replace(
                "/var/lib/wt/images/host.qcow2",
                "/var/lib/wt/images/devcontainer.qcow2"
            ))
            .unwrap_err(),
            "devcontainer and host images must use different files"
        );
    }
}
