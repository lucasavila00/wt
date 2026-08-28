use crate::{GuestTransport, TransportError};

use std::fmt;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use wt_world::WorldId;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct DomainName(String);

impl DomainName {
    pub(crate) fn parse(value: String) -> Option<Self> {
        let domain_name = Self(value);
        domain_name.world_id().map(|_| domain_name)
    }

    pub(crate) fn for_world(world_id: WorldId) -> Self {
        Self(format!("wt-{}", world_id.as_uuid().simple()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn world_id(&self) -> Option<WorldId> {
        let value = self.0.strip_prefix("wt-")?;
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return None;
        }
        uuid::Uuid::parse_str(value).ok().map(WorldId::from)
    }
}

impl fmt::Display for DomainName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineSpec {
    pub world_id: WorldId,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
}

#[derive(Clone)]
pub struct Machine {
    pub world_id: WorldId,
    pub guest_ip: String,
    pub transport: Arc<dyn GuestTransport>,
}

#[derive(Clone, Debug)]
pub enum MachineInspection {
    Missing,
    Running(Machine),
    Stopped { reason: Option<String> },
}

impl fmt::Debug for Machine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Machine")
            .field("world_id", &self.world_id)
            .field("guest_ip", &self.guest_ip)
            .field("transport", &"<guest transport>")
            .finish()
    }
}

pub trait MachineProvider: Clone + Send + Sync + 'static {
    fn image_path(&self) -> &Path;
    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError>;
    fn inspect(&self, world_id: WorldId) -> Result<MachineInspection, WorkerError>;
    fn start(&self, world_id: WorldId) -> Result<Machine, WorkerError>;
    fn stop(&self, world_id: WorldId) -> Result<(), WorkerError>;
    fn disk_usage(&self, world_id: WorldId) -> Result<u64, WorkerError>;
    fn delete(&self, world_id: WorldId) -> Result<(), WorkerError>;
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

impl From<TransportError> for WorkerError {
    fn from(error: TransportError) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_names_are_derived_from_world_ids() {
        let world_id =
            WorldId::from(uuid::Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap());

        assert_eq!(
            DomainName::for_world(world_id).as_str(),
            "wt-0123456789abcdef0123456789abcdef"
        );
        assert_eq!(DomainName::for_world(world_id).world_id(), Some(world_id));
    }

    #[test]
    fn rejects_non_wt_domain_names() {
        assert_eq!(DomainName::parse("other".into()), None);
        assert_eq!(DomainName::parse("wt-not-a-uuid".into()), None);
        assert_eq!(
            DomainName::parse("wt-01234567-89ab-cdef-0123-456789abcdef".into()),
            None
        );
    }
}
