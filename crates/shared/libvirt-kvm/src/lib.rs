mod config;
mod provider;
mod transport;
mod worker;

#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

pub use config::{
    CodexMounts, MachineConfig, CODEX_AUTH_TAG, CODEX_SESSIONS_TAG, GUEST_ARCHITECTURE,
    GUEST_MACHINE, LIBVIRT_URI,
};
pub use provider::*;
pub use transport::*;
pub use worker::LibvirtProvider;

pub const MACHINE_BOOTSTRAP_PACKAGES: &[&str] = &["qemu-guest-agent"];
