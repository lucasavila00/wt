use serde::Deserialize;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_PATH: &str = "/etc/wt/local.toml";

#[derive(Clone, Debug)]
pub struct LibvirtConfig {
    pub image: PathBuf,
    pub worlds_dir: PathBuf,
    pub libvirt_uri: String,
    pub network: String,
    pub architecture: String,
    pub machine: String,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub boot_timeout: std::time::Duration,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    image: Option<PathBuf>,
    worlds_dir: Option<PathBuf>,
    libvirt_uri: Option<String>,
    network: Option<String>,
    architecture: Option<String>,
    machine: Option<String>,
    memory_mib: Option<u64>,
    vcpus: Option<u32>,
    disk_gib: Option<u64>,
    boot_timeout_seconds: Option<u64>,
}

impl LibvirtConfig {
    pub fn from_env() -> Result<Self, String> {
        let config_path = std::env::var_os("WT_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
        let file = read_config(&config_path)?;

        let config = Self {
            image: env_path(
                "WT_IMAGE",
                file.image,
                "/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2",
            ),
            worlds_dir: env_path(
                "WT_WORLDS_DIR",
                file.worlds_dir,
                "/var/lib/libvirt/images/wt",
            ),
            libvirt_uri: env_string("WT_LIBVIRT_URI", file.libvirt_uri, "qemu:///system"),
            network: env_string("WT_LIBVIRT_NETWORK", file.network, "default"),
            architecture: env_string("WT_GUEST_ARCH", file.architecture, "x86_64"),
            machine: env_string("WT_GUEST_MACHINE", file.machine, "q35"),
            memory_mib: env_parse("WT_GUEST_MEMORY_MIB", file.memory_mib, 2048)?,
            vcpus: env_parse("WT_GUEST_VCPUS", file.vcpus, 2)?,
            disk_gib: env_parse("WT_GUEST_DISK_GIB", file.disk_gib, 16)?,
            boot_timeout: std::time::Duration::from_secs(env_parse(
                "WT_GUEST_BOOT_TIMEOUT_SECONDS",
                file.boot_timeout_seconds,
                300,
            )?),
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("libvirt_uri", self.libvirt_uri.as_str()),
            ("network", self.network.as_str()),
            ("architecture", self.architecture.as_str()),
            ("machine", self.machine.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        if self.memory_mib == 0 || self.vcpus == 0 || self.disk_gib == 0 {
            return Err("memory_mib, vcpus, and disk_gib must be greater than zero".to_owned());
        }
        if self.boot_timeout.is_zero() {
            return Err("boot_timeout_seconds must be greater than zero".to_owned());
        }
        Ok(())
    }
}

fn read_config(path: &Path) -> Result<FileConfig, String> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("read config {}: {error}", path.display()))?;
    toml::from_str(&contents).map_err(|error| format!("parse config {}: {error}", path.display()))
}

fn env_string(name: &str, file: Option<String>, default: &str) -> String {
    std::env::var(name)
        .ok()
        .or(file)
        .unwrap_or_else(|| default.to_owned())
}

fn env_path(name: &str, file: Option<PathBuf>, default: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .or(file)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn env_parse<T>(name: &str, file: Option<T>, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| format!("invalid {name}: {error}")),
        Err(_) => Ok(file.unwrap_or(default)),
    }
}
