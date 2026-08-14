mod transport;

pub use transport::{
    validate_executable, validate_file_path, CaptureRequest, CapturedOutput, GuestTransport,
    RunOutput, RunRequest, StreamKind, TransportError, WriteFileRequest,
};

use std::fmt;
use std::io::Write;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn parse(value: &str) -> Result<Self, WorkerError> {
        let suffix = value.strip_prefix("wt-").unwrap_or_default();
        if suffix.len() != 32
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(WorkerError::new(
                "provider ID must have the form wt- followed by 32 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineSpec {
    pub provider_id: ProviderId,
    pub disk_id: Uuid,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub cloud_init: NoCloudConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoCloudConfig {
    pub user_data: String,
    pub vendor_data: String,
}

impl Default for NoCloudConfig {
    fn default() -> Self {
        Self {
            user_data: "#cloud-config\n".to_owned(),
            vendor_data: "#cloud-config\n".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForkMachineSpec {
    pub source_provider_id: ProviderId,
    pub source_disk_id: Uuid,
    pub source_head_disk_id: Uuid,
    pub machine: MachineSpec,
}

#[derive(Clone)]
pub struct Machine {
    pub provider_id: ProviderId,
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
            .field("provider_id", &self.provider_id)
            .field("guest_ip", &self.guest_ip)
            .field("transport", &"<guest transport>")
            .finish()
    }
}

pub trait MachineProvider: Clone + Send + Sync + 'static {
    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError>;
    fn fork(&self, spec: &ForkMachineSpec, progress: &mut dyn Write) -> Result<Machine, ForkError>;
    fn inspect(&self, provider_id: &ProviderId) -> Result<MachineInspection, WorkerError>;
    fn start(&self, provider_id: &ProviderId) -> Result<Machine, WorkerError>;
    fn delete(&self, provider_id: &ProviderId, disk_ids: &[Uuid]) -> Result<(), WorkerError>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{error}")]
pub struct ForkError {
    error: WorkerError,
    source_pivoted: bool,
}

impl ForkError {
    pub fn before_pivot(error: WorkerError) -> Self {
        Self {
            error,
            source_pivoted: false,
        }
    }

    pub fn after_pivot(error: WorkerError) -> Self {
        Self {
            error,
            source_pivoted: true,
        }
    }

    pub fn source_pivoted(&self) -> bool {
        self.source_pivoted
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

impl From<TransportError> for WorkerError {
    fn from(error: TransportError) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_are_safe_stable_resource_names() {
        assert!(ProviderId::parse("wt-0123456789abcdef0123456789abcdef").is_ok());
        for invalid in [
            "../wt-0123456789abcdef0123456789abcdef",
            "wt-0123456789ABCDEF0123456789ABCDEF",
            "other-0123456789abcdef0123456789abcdef",
            "wt-short",
        ] {
            assert!(ProviderId::parse(invalid).is_err(), "accepted {invalid}");
        }
    }
}
