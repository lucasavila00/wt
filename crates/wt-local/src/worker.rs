use thiserror::Error;
use uuid::Uuid;
use wt_api::{InstanceName, SshEndpoint};

pub mod qemu;

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

#[derive(Clone, Debug, Default)]
pub struct FakeWorker {
    pub host: Option<String>,
    pub fail_provision: bool,
    pub fail_destroy: bool,
}

impl WorldWorker for FakeWorker {
    fn provision(&self, _spec: &ProvisionSpec<'_>) -> Result<SshEndpoint, WorkerError> {
        if self.fail_provision {
            return Err(WorkerError::new("injected provision failure"));
        }
        Ok(SshEndpoint {
            user: "ubuntu".to_owned(),
            host: self.host.clone().unwrap_or_else(|| "192.0.2.2".to_owned()),
            port: 22,
        })
    }

    fn destroy(&self, _backend_id: &str) -> Result<(), WorkerError> {
        if self.fail_destroy {
            return Err(WorkerError::new("injected destroy failure"));
        }
        Ok(())
    }

    fn inspect(&self, _backend_id: &str) -> Result<Option<SshEndpoint>, WorkerError> {
        Ok(Some(SshEndpoint {
            user: "ubuntu".to_owned(),
            host: self.host.clone().unwrap_or_else(|| "192.0.2.2".to_owned()),
            port: 22,
        }))
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
