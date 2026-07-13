mod config;
mod worker;

pub use config::{MachineConfig, GUEST_ARCHITECTURE, GUEST_MACHINE, LIBVIRT_URI};
pub use worker::LibvirtProvider;

pub const MACHINE_BOOTSTRAP_PACKAGES: &[&str] = &["qemu-guest-agent"];
pub use wt_provider::SessionFrontend;
