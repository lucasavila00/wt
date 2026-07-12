use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub const SERVER_CONFIG_PATH: &str = "/etc/wt/server.toml";
pub const LIBVIRT_URI: &str = "qemu:///system";
pub const GUEST_ARCHITECTURE: &str = "x86_64";
pub const GUEST_MACHINE: &str = "q35";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub version: u32,
    pub backend: BackendConfig,
    pub image: ImageConfig,
    pub libvirt: ServerLibvirtConfig,
    pub registry_cache: RegistryCacheConfig,
    pub git: GitConfig,
    pub guest: GuestConfig,
    pub install: InstallConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    pub kind: String,
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
pub struct GitConfig {
    pub identity_file: PathBuf,
    pub known_hosts_file: PathBuf,
}

/// Golden image path used by the server at runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageConfig {
    pub installed_path: PathBuf,
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
    pub session: SessionFrontend,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub boot_timeout_seconds: u64,
    pub recipe_timeout_seconds: u64,
    pub ssh_authorized_keys_file: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFrontend {
    Tmux,
    Byobu,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallConfig {
    pub binary_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LibvirtConfig {
    pub image: PathBuf,
    pub app_shell_binary: PathBuf,
    pub app_pane_binary: PathBuf,
    pub app_info_binary: PathBuf,
    pub app_proxy_binary: PathBuf,
    pub worlds_dir: PathBuf,
    pub network: String,
    pub registry_cache_state_dir: PathBuf,
    pub registry_cache_port: u16,
    pub git_identity_file: PathBuf,
    pub git_known_hosts_file: PathBuf,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub boot_timeout: Duration,
    pub recipe_timeout: Duration,
    pub ssh_authorized_keys: Vec<String>,
    pub session: SessionFrontend,
}

impl ServerConfig {
    pub fn load() -> Result<Self, String> {
        Self::load_from(Path::new(SERVER_CONFIG_PATH))
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("read config {}: {error}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .map_err(|error| format!("parse config {}: {error}", path.display()))?;
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
        if self.backend.kind != "libvirt" {
            return Err("backend.kind must be libvirt for a libvirt server config".to_owned());
        }
        let git_identity_file = expand_home(&self.git.identity_file, "git.identity_file")?;
        let git_known_hosts_file = expand_home(&self.git.known_hosts_file, "git.known_hosts_file")?;
        for (name, path) in [
            ("image.installed_path", &self.image.installed_path),
            ("libvirt.worlds_dir", &self.libvirt.worlds_dir),
            ("registry_cache.state_dir", &self.registry_cache.state_dir),
            ("install.binary_dir", &self.install.binary_dir),
            ("git.identity_file", &git_identity_file),
            ("git.known_hosts_file", &git_known_hosts_file),
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
        if self
            .image
            .installed_path
            .extension()
            .and_then(|value| value.to_str())
            != Some("qcow2")
        {
            return Err("image.installed_path must end in .qcow2".to_owned());
        }
        let image_dir = self
            .image
            .installed_path
            .parent()
            .ok_or_else(|| "image.installed_path must have a parent directory".to_owned())?;
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
            (
                "registry_cache.state_dir",
                self.registry_cache.state_dir.as_path(),
                "image directory",
                image_dir,
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
        if self.guest.memory_mib == 0
            || self.guest.vcpus == 0
            || self.guest.disk_gib == 0
            || self.guest.boot_timeout_seconds == 0
            || self.guest.recipe_timeout_seconds == 0
        {
            return Err("guest resource values must be greater than zero".to_owned());
        }
        let keys = self.ssh_authorized_keys()?;
        if keys.is_empty() {
            return Err(
                "guest.ssh_authorized_keys_file must contain at least one public key".to_owned(),
            );
        }
        for key in &keys {
            validate_public_key(key)?;
        }
        Ok(())
    }

    pub fn worker_config(&self) -> Result<LibvirtConfig, String> {
        let git = self.resolved_git_config()?;
        Ok(LibvirtConfig {
            image: self.image.installed_path.clone(),
            app_shell_binary: self.install.binary_dir.join("wt-app-shell"),
            app_pane_binary: self.install.binary_dir.join("wt-app-pane"),
            app_info_binary: self.install.binary_dir.join("wt-app-info"),
            app_proxy_binary: self.install.binary_dir.join("wt-app-proxy"),
            worlds_dir: self.libvirt.worlds_dir.clone(),
            network: self.libvirt.network.clone(),
            registry_cache_state_dir: self.registry_cache.state_dir.clone(),
            registry_cache_port: self.registry_cache.port,
            git_identity_file: git.identity_file,
            git_known_hosts_file: git.known_hosts_file,
            memory_mib: self.guest.memory_mib,
            vcpus: self.guest.vcpus,
            disk_gib: self.guest.disk_gib,
            boot_timeout: Duration::from_secs(self.guest.boot_timeout_seconds),
            recipe_timeout: Duration::from_secs(self.guest.recipe_timeout_seconds),
            ssh_authorized_keys: self.ssh_authorized_keys()?,
            session: self.guest.session,
        })
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

    pub fn ssh_authorized_keys(&self) -> Result<Vec<String>, String> {
        let path = expand_home(
            &self.guest.ssh_authorized_keys_file,
            "guest.ssh_authorized_keys_file",
        )?;
        let contents = std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "read guest.ssh_authorized_keys_file {}: {error}",
                path.display()
            )
        })?;
        Ok(contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn resolved_git_config(&self) -> Result<GitConfig, String> {
        Ok(GitConfig {
            identity_file: expand_home(&self.git.identity_file, "git.identity_file")?,
            known_hosts_file: expand_home(&self.git.known_hosts_file, "git.known_hosts_file")?,
        })
    }
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

fn expand_home(path: &Path, name: &str) -> Result<PathBuf, String> {
    if path == Path::new("~") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned())?;
        return Ok(home);
    }
    if let Some(relative) = path.to_str().and_then(|value| value.strip_prefix("~/")) {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned())?;
        return Ok(home.join(relative));
    }
    if !path.is_absolute() {
        return Err(format!("{name} must be absolute or start with ~/"));
    }
    Ok(path.to_owned())
}

fn validate_public_key(key: &str) -> Result<(), String> {
    if key.contains('\n') || key.contains('\r') || key.contains("PRIVATE KEY") {
        return Err("guest.ssh_authorized_keys_file accepts public keys only".to_owned());
    }
    let mut fields = key.split_whitespace();
    let kind = fields.next().unwrap_or_default();
    let data = fields.next().unwrap_or_default();
    let supported =
        kind == "ssh-ed25519" || kind == "ssh-rsa" || kind.starts_with("ecdsa-sha2-nistp");
    if !supported
        || data.len() < 16
        || !data.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/' || byte == b'='
        })
    {
        return Err("guest.ssh_authorized_keys_file contains an invalid public key".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
version = 1

[backend]
kind = "libvirt"

[image]
installed_path = "/var/lib/wt/images/wt.qcow2"

[libvirt]
network = "default"
worlds_dir = "/var/lib/libvirt/images/wt"

[registry_cache]
state_dir = "/var/lib/wt/registry-cache"
port = 3128
max_size_gib = 64
registries = ["docker.io", "mcr.microsoft.com"]

[git]
identity_file = "/tmp/wt-test-git-identity"
known_hosts_file = "/tmp/wt-test-git-known-hosts"

[guest]
session = "tmux"
memory_mib = 8192
vcpus = 4
disk_gib = 32
boot_timeout_seconds = 300
recipe_timeout_seconds = 900
ssh_authorized_keys_file = "KEY_FILE"

[install]
binary_dir = "/usr/local/bin"
"#;

    fn parse(value: &str) -> Result<(ServerConfig, LibvirtConfig), String> {
        let key_dir = tempfile::tempdir().unwrap();
        let key_file = key_dir.path().join("id.pub");
        std::fs::write(
            &key_file,
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestOnlyKeyMaterial wt@example\n",
        )
        .unwrap();
        let value = value.replace("KEY_FILE", key_file.to_str().unwrap());
        let config: ServerConfig = toml::from_str(&value).map_err(|error| error.to_string())?;
        config.validate()?;
        let worker = config.worker_config()?;
        Ok((config, worker))
    }

    #[test]
    fn complete_config_is_valid() {
        let (_, worker) = parse(VALID).unwrap();
        assert_eq!(
            worker.app_shell_binary,
            Path::new("/usr/local/bin/wt-app-shell")
        );
        assert_eq!(
            worker.app_pane_binary,
            Path::new("/usr/local/bin/wt-app-pane")
        );
        assert_eq!(worker.registry_cache_port, 3128);
        assert_eq!(worker.session, SessionFrontend::Tmux);
    }

    #[test]
    fn missing_and_unknown_fields_fail() {
        assert!(parse(&VALID.replace("vcpus = 4\n", "")).is_err());
        assert!(parse(&VALID.replace("session = \"tmux\"\n", "")).is_err());
        assert!(parse(&VALID.replace("vcpus = 4", "vcpus = 4\nfallback = true")).is_err());
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
        assert!(parse(&VALID.replace("vcpus = 4", "vcpus = 0")).is_err());
        assert!(parse(&VALID.replace("session = \"tmux\"", "session = \"screen\"")).is_err());
        assert!(parse(&VALID.replace("max_size_gib = 64", "max_size_gib = 0")).is_err());
        assert!(parse(
            &VALID.replace("/tmp/wt-test-git-identity", "relative/wt-test-git-identity")
        )
        .is_err());
        assert!(parse(&VALID.replace(
            "installed_path = \"/var/lib/wt/images/wt.qcow2\"",
            "installed_path = \"/var/lib/wt/images/wt.qcow2\"\nsource_url = \"https://example.com/img\""
        ))
        .is_err());
    }

    #[test]
    fn git_paths_expand_home() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(
            expand_home(Path::new("~/.ssh/id_ed25519"), "git.identity_file").unwrap(),
            home.join(".ssh/id_ed25519")
        );
    }
}
