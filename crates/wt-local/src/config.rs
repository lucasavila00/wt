use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct LocalConfig {
    pub state_dir: PathBuf,
    pub image: PathBuf,
    pub worlds_dir: PathBuf,
    pub libvirt_uri: String,
    pub network: String,
    pub guest_user: String,
    pub ssh_public_key: PathBuf,
    pub ssh_private_key: PathBuf,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub boot_timeout: std::time::Duration,
}

impl LocalConfig {
    pub fn from_env() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned())?;
        let repo_image = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("imgs/ubuntu-24.04-server-cloudimg-amd64.img");
        let state_dir = std::env::var_os("WT_STATE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_STATE_HOME").map(|path| PathBuf::from(path).join("wt"))
            })
            .unwrap_or_else(|| home.join(".local/state/wt"));

        Ok(Self {
            image: env_path("WT_IMAGE", repo_image),
            worlds_dir: env_path("WT_WORLDS_DIR", state_dir.join("worlds")),
            ssh_public_key: env_path("WT_SSH_PUBLIC_KEY", home.join(".ssh/id_ed25519.pub")),
            ssh_private_key: env_path("WT_SSH_PRIVATE_KEY", home.join(".ssh/id_ed25519")),
            state_dir,
            libvirt_uri: env_value("WT_LIBVIRT_URI", "qemu:///system"),
            network: env_value("WT_LIBVIRT_NETWORK", "default"),
            guest_user: env_value("WT_GUEST_USER", "ubuntu"),
            memory_mib: env_parse("WT_GUEST_MEMORY_MIB", 2048)?,
            vcpus: env_parse("WT_GUEST_VCPUS", 2)?,
            disk_gib: env_parse("WT_GUEST_DISK_GIB", 16)?,
            boot_timeout: std::time::Duration::from_secs(env_parse(
                "WT_GUEST_BOOT_TIMEOUT_SECONDS",
                900,
            )?),
        })
    }

    pub fn database_path(&self) -> PathBuf {
        self.state_dir.join("instances.db")
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
