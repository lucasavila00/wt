use std::path::PathBuf;
use std::time::Duration;

pub const LIBVIRT_URI: &str = "qemu:///system";
pub const GUEST_ARCHITECTURE: &str = "x86_64";
pub const GUEST_MACHINE: &str = "q35";

#[derive(Clone, Debug)]
pub struct MachineConfig {
    pub image: PathBuf,
    pub worlds_dir: PathBuf,
    pub network: String,
    pub boot_timeout: Duration,
    pub shared_folders: Vec<SharedFolder>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedFolder {
    pub source: PathBuf,
    pub tag: String,
}
