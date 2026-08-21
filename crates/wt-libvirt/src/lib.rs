mod config;
mod worker;

pub use config::{
    CodexMounts, MachineConfig, CODEX_AUTH_TAG, CODEX_SESSIONS_TAG, GUEST_ARCHITECTURE,
    GUEST_MACHINE, LIBVIRT_URI,
};
pub use worker::LibvirtProvider;

pub const MACHINE_BOOTSTRAP_PACKAGES: &[&str] = &["qemu-guest-agent"];
