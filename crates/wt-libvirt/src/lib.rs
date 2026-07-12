mod config;
mod worker;

pub use config::{
    GitConfig, GuestConfig, ImageConfig, InstallConfig, LibvirtConfig, RegistryCacheConfig,
    ServerConfig, ServerLibvirtConfig, SessionFrontend, GUEST_ARCHITECTURE, GUEST_MACHINE,
    LIBVIRT_URI, SERVER_CONFIG_PATH,
};
pub use worker::LibvirtWorker;

use thiserror::Error;
use uuid::Uuid;
use wt_api::{AppSshAccess, InstanceName, SshAccess};

#[derive(Clone, Debug)]
pub struct ProvisionSpec<'a> {
    pub id: Uuid,
    pub backend_id: &'a str,
    pub owner: &'a str,
    pub name: &'a InstanceName,
    pub source: &'a str,
    pub git_passphrase: &'a wt_api::GitPassphrase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    pub guest_ip: String,
    pub ssh: SshAccess,
    pub app_ssh: AppSshAccess,
}

pub trait WorldWorker {
    fn validate_git_passphrase(
        &self,
        passphrase: &wt_api::GitPassphrase,
    ) -> Result<(), WorkerError>;
    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn std::io::Write,
    ) -> Result<World, WorkerError>;
    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError>;
    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct WorkerError {
    message: String,
}

impl WorkerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
