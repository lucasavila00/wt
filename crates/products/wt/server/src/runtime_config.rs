use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use wt_libvirt_kvm::{MachineConfig, SharedMounts};

pub const DEFAULT_AGENT_TOOL_VSOCK_PORT: u32 = wt_agent_tool_gateway::VSOCK_PORT;
pub const AGENT_TOOL_VSOCK_PORT_ENV: &str = wt_agent_tool_gateway::VSOCK_PORT_ENV;

pub const SERVER_CONFIG_PATH: &str = "/etc/wt/server.toml";
pub const CODEX_AUTH_PATH: &str = "/home/wt/.codex/auth.json";
pub const CODEX_AUTH_SHARE_DIR: &str = "/home/wt/.codex/.wt-auth";
pub const CODEX_SESSIONS_PATH: &str = "/home/wt/.codex/sessions";
pub const TEST_CODEX_AUTH_PATH: &str = "/home/wt/.config/wt/kvm-test/codex/auth.json";
pub const TEST_CODEX_AUTH_SHARE_DIR: &str = "/home/wt/.config/wt/kvm-test/codex/auth-share";
pub const TEST_CODEX_SESSIONS_PATH: &str = "/home/wt/.config/wt/kvm-test/codex/sessions";
pub const SSH_AUTHORIZED_KEYS_SHARE_DIR: &str = "/home/wt/.ssh/.wt-authorized-keys";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodexPaths {
    pub auth: &'static str,
    pub auth_share: &'static str,
    pub sessions: &'static str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub version: u32,
    #[serde(default)]
    pub test_server: bool,
    pub image: ImageConfig,
    pub libvirt: ServerLibvirtConfig,
    pub agent_tools: AgentToolsConfig,
    pub guest: GuestConfig,
    pub install: InstallConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentToolsConfig {
    #[serde(default = "default_agent_tools_vsock_port")]
    pub vsock_port: u32,
    pub github: Option<AgentToolsProviderConfig>,
    pub gitlab: Option<AgentToolsProviderConfig>,
}

fn default_agent_tools_vsock_port() -> u32 {
    DEFAULT_AGENT_TOOL_VSOCK_PORT
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentToolsProviderConfig {
    pub host: String,
}

/// Golden image path used by the server at runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageConfig {
    pub path: PathBuf,
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
    pub readiness_timeout_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallConfig {
    pub binary_dir: PathBuf,
}

impl ServerConfig {
    pub fn load() -> Result<Self, String> {
        let config = Self::load_runtime_from(Path::new(SERVER_CONFIG_PATH))?;
        config.validate_codex_sources()?;
        Ok(config)
    }

    pub fn load_runtime_from(path: &Path) -> Result<Self, String> {
        let mut config = Self::load_from(path)?;
        let generation = crate::image_generation::resolve(&config.image.path)?;
        config.image.path = generation.image;
        Ok(config)
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("read config {}: {error}", path.display()))?;
        let mut config: Self = toml::from_str(&contents)
            .map_err(|error| format!("parse config {}: {error}", path.display()))?;
        if let Some(port) =
            wt_agent_tool_gateway::vsock_port_from_env().map_err(|error| error.to_string())?
        {
            config.agent_tools.vsock_port = port;
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
            ("image.path", &self.image.path),
            ("libvirt.worlds_dir", &self.libvirt.worlds_dir),
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
        for (name, path) in [("image.path", &self.image.path)] {
            if path.extension().and_then(|value| value.to_str()) != Some("qcow2") {
                return Err(format!("{name} must end in .qcow2"));
            }
        }
        let image_dir = self
            .image
            .path
            .parent()
            .ok_or_else(|| "image.path must have a parent directory".to_owned())?;
        for (left_name, left, right_name, right) in [
            (
                "image directory",
                image_dir,
                "libvirt.worlds_dir",
                self.libvirt.worlds_dir.as_path(),
            ),
            (
                "image directory",
                image_dir,
                "install.binary_dir",
                self.install.binary_dir.as_path(),
            ),
            (
                "libvirt.worlds_dir",
                self.libvirt.worlds_dir.as_path(),
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
        self.validate_agent_tools()?;
        if self.guest.boot_timeout_seconds == 0 || self.guest.readiness_timeout_seconds == 0 {
            return Err("guest timeout values must be greater than zero".to_owned());
        }
        Ok(())
    }

    pub fn machine_config(&self) -> MachineConfig {
        MachineConfig {
            image: self.image.path.clone(),
            worlds_dir: self.libvirt.worlds_dir.clone(),
            worlds_owner_uid: wt_retained_worlds::WT_IDENTITY.uid,
            network: self.libvirt.network.clone(),
            boot_timeout: Duration::from_secs(self.guest.boot_timeout_seconds),
            shared_mounts: Some(self.shared_mounts()),
        }
    }

    pub fn retained_config(&self) -> wt_retained_worlds::RetainedConfig {
        wt_retained_worlds::RetainedConfig {
            agent_tools: wt_retained_worlds::AgentToolsConfig {
                provider_hosts: self.agent_tools_provider_hosts(),
                vsock_port: self.agent_tools.vsock_port,
            },
        }
    }

    pub fn codex_paths(&self) -> CodexPaths {
        if self.test_server {
            CodexPaths {
                auth: TEST_CODEX_AUTH_PATH,
                auth_share: TEST_CODEX_AUTH_SHARE_DIR,
                sessions: TEST_CODEX_SESSIONS_PATH,
            }
        } else {
            CodexPaths {
                auth: CODEX_AUTH_PATH,
                auth_share: CODEX_AUTH_SHARE_DIR,
                sessions: CODEX_SESSIONS_PATH,
            }
        }
    }

    pub fn validate_codex_sources(&self) -> Result<(), String> {
        self.validate_codex_login()?;
        let paths = self.codex_paths();
        let sessions = Path::new(paths.sessions);
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
        let auth = Path::new(paths.auth);
        let share = Path::new(paths.auth_share);
        let shared_auth = share.join("auth.json");
        let shared_metadata = std::fs::symlink_metadata(&shared_auth).map_err(|error| {
            format!(
                "inspect Codex authentication share {}: {error}",
                shared_auth.display()
            )
        })?;
        if shared_metadata.file_type().is_symlink() || !shared_metadata.is_file() {
            return Err(format!(
                "Codex authentication share must contain a regular copy of {}: {}",
                auth.display(),
                shared_auth.display()
            ));
        }
        let auth_contents = std::fs::read(auth).map_err(|error| {
            format!("read Codex authentication file {}: {error}", auth.display())
        })?;
        let shared_contents = std::fs::read(&shared_auth).map_err(|error| {
            format!(
                "read Codex authentication share {}: {error}",
                shared_auth.display()
            )
        })?;
        if auth_contents != shared_contents {
            return Err(format!(
                "Codex authentication share does not match {}: {}",
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
        let auth = Path::new(self.codex_paths().auth);
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

    fn shared_mounts(&self) -> SharedMounts {
        let paths = self.codex_paths();
        SharedMounts {
            sessions: PathBuf::from(paths.sessions),
            auth: PathBuf::from(paths.auth_share),
            ssh_authorized_keys: PathBuf::from(SSH_AUTHORIZED_KEYS_SHARE_DIR),
        }
    }

    fn agent_tools_provider_hosts(&self) -> Vec<String> {
        [
            self.agent_tools.github.as_ref(),
            self.agent_tools.gitlab.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|provider| provider.host.clone())
        .collect()
    }

    fn validate_agent_tools(&self) -> Result<(), String> {
        if self.agent_tools.vsock_port == 0 || self.agent_tools.vsock_port == u32::MAX {
            return Err("agent_tools.vsock_port must be a concrete nonzero port".to_owned());
        }
        let providers = [
            ("agent_tools.github.host", self.agent_tools.github.as_ref()),
            ("agent_tools.gitlab.host", self.agent_tools.gitlab.as_ref()),
        ];
        if providers.iter().all(|(_, provider)| provider.is_none()) {
            return Err("at least one agent tool provider is required".to_owned());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const VALID: &str = r#"
version = 1

[image]
path = "/var/lib/wt/images/retained.qcow2"

[libvirt]
network = "default"
worlds_dir = "/var/lib/libvirt/images/wt"

[agent_tools.github]
host = "github.com"

[guest]
boot_timeout_seconds = 300
readiness_timeout_seconds = 900

[install]
binary_dir = "/usr/local/bin"
"#;

    fn parse(value: &str) -> Result<(ServerConfig, MachineConfig), String> {
        let config: ServerConfig = toml::from_str(value).map_err(|error| error.to_string())?;
        config.validate()?;
        let machine = config.machine_config();
        Ok((config, machine))
    }

    #[test]
    fn complete_config_is_valid() {
        let (config, machine) = parse(VALID).unwrap();
        assert_eq!(config.agent_tools.vsock_port, DEFAULT_AGENT_TOOL_VSOCK_PORT);
        assert_eq!(
            machine.image,
            Path::new("/var/lib/wt/images/retained.qcow2")
        );
        assert_eq!(machine.network, "default");
        assert_eq!(
            machine.shared_mounts,
            Some(SharedMounts {
                sessions: PathBuf::from(CODEX_SESSIONS_PATH),
                auth: PathBuf::from(CODEX_AUTH_SHARE_DIR),
                ssh_authorized_keys: PathBuf::from(SSH_AUTHORIZED_KEYS_SHARE_DIR),
            })
        );
    }

    #[test]
    fn test_server_uses_isolated_codex_paths() {
        let (config, machine) = parse(&format!("test_server = true\n{VALID}")).unwrap();

        assert_eq!(
            config.codex_paths(),
            CodexPaths {
                auth: TEST_CODEX_AUTH_PATH,
                auth_share: TEST_CODEX_AUTH_SHARE_DIR,
                sessions: TEST_CODEX_SESSIONS_PATH,
            }
        );
        assert_eq!(
            machine.shared_mounts.unwrap(),
            SharedMounts {
                sessions: PathBuf::from(TEST_CODEX_SESSIONS_PATH),
                auth: PathBuf::from(TEST_CODEX_AUTH_SHARE_DIR),
                ssh_authorized_keys: PathBuf::from(SSH_AUTHORIZED_KEYS_SHARE_DIR),
            }
        );
    }

    #[test]
    fn runtime_load_pins_one_image_generation() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("retained.qcow2");
        let generations = crate::image_generation::generations_path(&image);
        let first = generations.join("first");
        let second = generations.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let current = crate::image_generation::current_path(&image);
        symlink("retained.qcow2.generations/first", &current).unwrap();
        let config_path = directory.path().join("server.toml");
        std::fs::write(
            &config_path,
            VALID.replace(
                "/var/lib/wt/images/retained.qcow2",
                &image.display().to_string(),
            ),
        )
        .unwrap();

        let pinned = ServerConfig::load_runtime_from(&config_path).unwrap();
        std::fs::remove_file(&current).unwrap();
        symlink("retained.qcow2.generations/second", &current).unwrap();
        let refreshed = ServerConfig::load_runtime_from(&config_path).unwrap();

        assert_eq!(pinned.image.path, first.join("retained.qcow2"));
        assert_eq!(refreshed.image.path, second.join("retained.qcow2"));
    }

    #[test]
    fn missing_and_unknown_fields_fail() {
        assert!(parse(&VALID.replace("readiness_timeout_seconds = 900\n", "")).is_err());
        assert!(parse(&VALID.replace(
            "readiness_timeout_seconds = 900",
            "readiness_timeout_seconds = 900\nfallback = true"
        ))
        .is_err());
        assert!(parse(&VALID.replace(
            "[agent_tools.github]",
            "[agent_tools]\nunknown = true\n\n[agent_tools.github]"
        ))
        .is_err());
    }

    #[test]
    fn invalid_values_fail() {
        assert!(parse(&VALID.replace("/usr/local/bin", "relative/bin")).is_err());
        assert!(parse(&VALID.replace("/usr/local/bin", "/")).is_err());
        assert!(parse(&VALID.replace("/usr/local/bin", "/usr/../bin")).is_err());
        assert!(parse(&VALID.replace("/usr/local/bin", "/var/lib/wt")).is_err());
        assert!(parse(&VALID.replace(
            "readiness_timeout_seconds = 900",
            "readiness_timeout_seconds = 0"
        ))
        .is_err());
        assert!(parse(&VALID.replace(
            "[agent_tools.github]",
            "[agent_tools]\nvsock_port = 0\n\n[agent_tools.github]"
        ))
        .is_err());
        assert!(parse(&VALID.replace(
            "path = \"/var/lib/wt/images/retained.qcow2\"",
            "path = \"/var/lib/wt/images/retained.qcow2\"\nsource_url = \"https://example.com/img\""
        ))
        .is_err());
    }
}
