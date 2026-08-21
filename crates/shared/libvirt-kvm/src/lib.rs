mod config;
mod provider;
mod transport;
mod worker;

pub use config::{
    CodexMounts, MachineConfig, CODEX_AUTH_TAG, CODEX_SESSIONS_TAG, GUEST_ARCHITECTURE,
    GUEST_MACHINE, LIBVIRT_URI,
};
pub use worker::LibvirtProvider;
pub use provider::*;
pub use transport::*;

pub const MACHINE_BOOTSTRAP_PACKAGES: &[&str] = &["qemu-guest-agent"];
