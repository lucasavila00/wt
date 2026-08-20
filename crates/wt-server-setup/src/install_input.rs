use serde::Deserialize;
use std::path::{Path, PathBuf};
use wt_server::{
    AgentGitConfig, AgentGitProviderConfig, GuestConfig, ImageConfig, InstallConfig,
    RegistryCacheConfig, ServerConfig, ServerLibvirtConfig, SharedFolder,
    DEFAULT_AGENT_GIT_VSOCK_PORT,
};
use wt_setup_core::expand_home;

/// Install input for `wt-server-setup --config`.
/// Setup materializes [`ServerConfig`] from this and writes `/etc/wt/server.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallInput {
    pub version: u32,
    #[serde(default)]
    pub shared_folders: Vec<SharedFolder>,
    pub capacity: wt_registry::CapacityConfig,
    pub image: InstallImageConfig,
    pub libvirt: ServerLibvirtConfig,
    pub registry_cache: RegistryCacheConfig,
    pub agent_git: AgentGitInstallConfig,
    pub guest: GuestConfig,
    pub install: InstallConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallImageConfig {
    pub source_url: String,
    pub source_sha256: String,
    pub devcontainer_path: PathBuf,
    pub host_path: PathBuf,
    pub build_memory_mib: u64,
    pub build_vcpus: u32,
    pub build_disk_gib: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentGitInstallConfig {
    #[serde(default = "default_agent_git_vsock_port")]
    pub vsock_port: u32,
    pub github: Option<AgentGitProviderInstallConfig>,
    pub gitlab: Option<AgentGitProviderInstallConfig>,
}

fn default_agent_git_vsock_port() -> u32 {
    DEFAULT_AGENT_GIT_VSOCK_PORT
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentGitProviderInstallConfig {
    pub host: String,
    pub api_token_file: PathBuf,
    pub ssh_private_key_file: PathBuf,
    pub ssh_public_key_file: PathBuf,
    pub ssh_known_hosts_file: PathBuf,
}

impl InstallInput {
    pub(crate) fn load_from(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("read install input {}: {error}", path.display()))?;
        let mut input: Self = toml::from_str(&contents)
            .map_err(|error| format!("parse install input {}: {error}", path.display()))?;
        input.resolve_paths()?;
        input.validate()?;
        Ok(input)
    }

    fn resolve_paths(&mut self) -> Result<(), String> {
        for (kind, provider) in self.agent_git.providers_mut() {
            for (field, path) in [
                ("api_token_file", &mut provider.api_token_file),
                ("ssh_private_key_file", &mut provider.ssh_private_key_file),
                ("ssh_public_key_file", &mut provider.ssh_public_key_file),
                ("ssh_known_hosts_file", &mut provider.ssh_known_hosts_file),
            ] {
                *path = expand_home(path, &format!("agent_git.{kind}.{field}"))?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported install input version {}; expected 1",
                self.version
            ));
        }
        if !self.image.source_url.starts_with("https://") {
            return Err("image.source_url must be an https URL".to_owned());
        }
        if self.image.source_sha256.len() != 64
            || !self
                .image
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("image.source_sha256 must contain 64 hexadecimal characters".to_owned());
        }
        if self.image.build_memory_mib == 0
            || self.image.build_vcpus == 0
            || self.image.build_disk_gib == 0
        {
            return Err("image build resource values must be greater than zero".to_owned());
        }
        self.capacity.validate()?;
        self.materialize().validate()
    }

    pub(crate) fn materialize(&self) -> ServerConfig {
        ServerConfig {
            version: self.version,
            shared_folders: self.shared_folders.clone(),
            image: ImageConfig {
                devcontainer_path: self.image.devcontainer_path.clone(),
                host_path: self.image.host_path.clone(),
            },
            libvirt: self.libvirt.clone(),
            registry_cache: self.registry_cache.clone(),
            agent_git: AgentGitConfig {
                vsock_port: self.agent_git.vsock_port,
                github: self
                    .agent_git
                    .github
                    .as_ref()
                    .map(|provider| AgentGitProviderConfig {
                        host: provider.host.clone(),
                    }),
                gitlab: self
                    .agent_git
                    .gitlab
                    .as_ref()
                    .map(|provider| AgentGitProviderConfig {
                        host: provider.host.clone(),
                    }),
            },
            guest: self.guest.clone(),
            install: self.install.clone(),
        }
    }

    pub(crate) fn source_url(&self) -> &str {
        &self.image.source_url
    }

    pub(crate) fn source_sha256(&self) -> &str {
        &self.image.source_sha256
    }
}

impl AgentGitInstallConfig {
    pub(crate) fn providers(
        &self,
    ) -> impl Iterator<Item = (&'static str, &AgentGitProviderInstallConfig)> {
        [
            ("github", self.github.as_ref()),
            ("gitlab", self.gitlab.as_ref()),
        ]
        .into_iter()
        .filter_map(|(kind, provider)| provider.map(|provider| (kind, provider)))
    }

    fn providers_mut(
        &mut self,
    ) -> impl Iterator<Item = (&'static str, &mut AgentGitProviderInstallConfig)> {
        [
            ("github", self.github.as_mut()),
            ("gitlab", self.gitlab.as_mut()),
        ]
        .into_iter()
        .filter_map(|(kind, provider)| provider.map(|provider| (kind, provider)))
    }
}

/// Serialize `ServerConfig` for `/etc/wt/server.toml` and image provenance.
pub(crate) fn serialize_server_config(config: &ServerConfig) -> Result<Vec<u8>, String> {
    let text = toml::to_string_pretty(config)
        .map_err(|error| format!("serialize server config: {error}"))?;
    Ok(text.into_bytes())
}

pub(crate) fn serialize_capacity_config(
    config: &wt_registry::CapacityConfig,
) -> Result<Vec<u8>, String> {
    let text = toml::to_string_pretty(config)
        .map_err(|error| format!("serialize capacity config: {error}"))?;
    Ok(text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
version = 1

[[shared_folders]]
source = "/var/lib/wt/shared/codex-sessions"
target = ".codex/sessions"

[[shared_folders]]
source = "/var/lib/wt/shared/claude-projects"
target = ".claude/projects"

[capacity]
version = 1
limits = { vcpus = 32, memory_mib = 131072, disk_gib = 2048 }

[image]
source_url = "https://cloud-images.ubuntu.com/image.img"
source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
devcontainer_path = "/var/lib/wt/images/devcontainer.qcow2"
host_path = "/var/lib/wt/images/host.qcow2"
build_memory_mib = 8192
build_vcpus = 4
build_disk_gib = 32

[libvirt]
network = "default"
worlds_dir = "/var/lib/libvirt/images/wt"

[registry_cache]
state_dir = "/var/lib/wt/registry-cache"
port = 3128
max_size_gib = 64
registries = ["docker.io"]

[agent_git.github]
host = "github.com"
api_token_file = "/tmp/github.token"
ssh_private_key_file = "/tmp/id_ed25519"
ssh_public_key_file = "/tmp/id_ed25519.pub"
ssh_known_hosts_file = "/tmp/known_hosts"

[guest]
boot_timeout_seconds = 300
recipe_timeout_seconds = 900

[install]
binary_dir = "/usr/local/bin"
"#;

    fn parse(value: &str) -> Result<InstallInput, String> {
        let input: InstallInput = toml::from_str(value).map_err(|error| error.to_string())?;
        input.validate()?;
        Ok(input)
    }

    #[test]
    fn materialize_drops_image_source_fields() {
        let input = parse(VALID).unwrap();
        let server = input.materialize();
        assert_eq!(
            server.image.devcontainer_path,
            PathBuf::from("/var/lib/wt/images/devcontainer.qcow2")
        );
        assert_eq!(server.shared_folders, input.shared_folders);
        let bytes = serialize_server_config(&server).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        insta::assert_snapshot!("materialized_server_config", text);
    }

    #[test]
    fn capacity_config_is_materialized_separately() {
        let input: InstallInput = toml::from_str(VALID).unwrap();
        let text = String::from_utf8(serialize_capacity_config(&input.capacity).unwrap()).unwrap();
        insta::assert_snapshot!(text, @r###"
        version = 1

        [limits]
        vcpus = 32
        memory_mib = 131072
        disk_gib = 2048
        "###);
    }

    #[test]
    fn invalid_source_fields_fail() {
        assert!(parse(&VALID.replace("https://", "http://")).is_err());
        assert!(parse(&VALID.replace(&"a".repeat(64), "not-a-sha")).is_err());
    }

    #[test]
    fn materialize_round_trips_as_server_config() {
        let input = parse(VALID).unwrap();
        let server = input.materialize();
        let bytes = serialize_server_config(&server).unwrap();
        let reloaded: ServerConfig = toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        reloaded.validate().unwrap();
        assert_eq!(reloaded, server);
    }

    #[test]
    fn example_install_inputs_are_valid() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in [
            "wt-server.development.toml",
            "wt-server.kvm-e2e-install.toml",
        ] {
            InstallInput::load_from(&workspace.join("examples/server-config").join(name)).unwrap();
        }
        ServerConfig::load_from(&workspace.join("examples/server-config/wt-server.kvm-test.toml"))
            .unwrap();
    }
}
