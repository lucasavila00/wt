use std::io::Write;
use uuid::Uuid;
use wt_libvirt_kvm::WorkerError;

#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

pub mod host;
mod retained;

pub use host::{ProvisionSpec, WorldInspection};
pub use retained::*;

pub trait WorldWorker: Clone + Send + Sync + 'static {
    fn provision(
        &self,
        spec: ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<GuestAccess, WorkerError>;
    fn destroy(&self, backend_id: &str, disk_id: Uuid) -> Result<(), WorkerError>;
    fn inspect(&self, backend_id: &str) -> Result<WorldInspection, WorkerError>;
    fn start(&self, backend_id: &str) -> Result<GuestAccess, WorkerError>;
    fn stop(&self, backend_id: &str) -> Result<(), WorkerError>;
    fn disk_usage(&self, disk_id: Uuid) -> Result<u64, WorkerError>;
}
