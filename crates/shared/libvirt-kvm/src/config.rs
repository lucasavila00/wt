use std::path::PathBuf;
use std::time::Duration;

pub const LIBVIRT_URI: &str = "qemu:///system";
pub const GUEST_ARCHITECTURE: &str = "x86_64";
pub const GUEST_MACHINE: &str = "q35";
pub const CODEX_SESSIONS_TAG: &str = "wt-codex-integration-sessions";
pub const CODEX_AUTH_TAG: &str = "wt-codex-integration-auth";

#[derive(Clone, Debug)]
pub struct MachineConfig {
    pub image: PathBuf,
    pub worlds_dir: PathBuf,
    pub worlds_owner_uid: u32,
    pub network: String,
    pub boot_timeout: Duration,
    pub codex_mounts: Option<CodexMounts>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexMounts {
    pub sessions: PathBuf,
    pub auth: PathBuf,
}
