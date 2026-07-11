mod config;
mod worker;

pub use config::{
    GuestConfig, ImageConfig, InstallConfig, LibvirtConfig, SiteConfig, SiteLibvirtConfig,
    GUEST_ARCHITECTURE, GUEST_MACHINE, LIBVIRT_URI, SITE_CONFIG_PATH,
};
pub use worker::LibvirtWorker;

use thiserror::Error;
use uuid::Uuid;
use wt_api::InstanceName;

#[derive(Clone, Debug)]
pub struct ProvisionSpec<'a> {
    pub id: Uuid,
    pub backend_id: &'a str,
    pub owner: &'a str,
    pub name: &'a InstanceName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    pub guest_ip: String,
}

pub trait WorldWorker {
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError>;
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
