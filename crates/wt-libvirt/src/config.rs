use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct LibvirtConfig {
    pub image: PathBuf,
    pub worlds_dir: PathBuf,
    pub libvirt_uri: String,
    pub network: String,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub boot_timeout: std::time::Duration,
}

impl LibvirtConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            image: env_path(
                "WT_IMAGE",
                PathBuf::from("/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2"),
            ),
            worlds_dir: env_path("WT_WORLDS_DIR", PathBuf::from("/var/lib/libvirt/images/wt")),
            libvirt_uri: env_value("WT_LIBVIRT_URI", "qemu:///system"),
            network: env_value("WT_LIBVIRT_NETWORK", "default"),
            memory_mib: env_parse("WT_GUEST_MEMORY_MIB", 2048)?,
            vcpus: env_parse("WT_GUEST_VCPUS", 2)?,
            disk_gib: env_parse("WT_GUEST_DISK_GIB", 16)?,
            boot_timeout: std::time::Duration::from_secs(env_parse(
                "WT_GUEST_BOOT_TIMEOUT_SECONDS",
                300,
            )?),
        })
    }
}

fn env_value(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn env_path(name: &str, default: PathBuf) -> PathBuf {
    std::env::var_os(name).map(PathBuf::from).unwrap_or(default)
}

fn env_parse<T>(name: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| format!("invalid {name}: {error}")),
        Err(_) => Ok(default),
    }
}
