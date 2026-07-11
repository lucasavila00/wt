use thiserror::Error;
use uuid::Uuid;
use wt_api::{InstanceName, SshEndpoint};

pub mod libvirt;

#[derive(Clone, Debug)]
pub struct ProvisionSpec<'a> {
    pub id: Uuid,
    pub backend_id: &'a str,
    pub owner: &'a str,
    pub name: &'a InstanceName,
}

pub trait WorldWorker {
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<SshEndpoint, WorkerError>;
    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError>;
    fn inspect(&self, backend_id: &str) -> Result<Option<SshEndpoint>, WorkerError>;
}

impl<T: WorldWorker + ?Sized> WorldWorker for Box<T> {
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<SshEndpoint, WorkerError> {
        (**self).provision(spec)
    }

    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError> {
        (**self).destroy(backend_id)
    }

    fn inspect(&self, backend_id: &str) -> Result<Option<SshEndpoint>, WorkerError> {
        (**self).inspect(backend_id)
    }
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
