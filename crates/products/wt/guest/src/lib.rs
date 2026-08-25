use std::io::Write;
use std::time::Duration;
use wt_libvirt_kvm::WorkerError;
use wt_world::WorldId;

#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

mod guest;
pub mod host;

pub use guest::*;
pub use host::{WorldInspection, WorldProvisionSpec};

fn write_creation_timing(
    log: &mut dyn Write,
    phase: &str,
    elapsed: Duration,
) -> Result<(), WorkerError> {
    writeln!(
        log,
        "World creation timing: {phase} took {:.3}s",
        elapsed.as_secs_f64()
    )
    .map_err(|error| WorkerError::new(format!("write world creation timing: {error}")))
}

pub trait WorldWorker: Clone + Send + Sync + 'static {
    fn provision(
        &self,
        spec: WorldProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<GuestAccess, WorkerError>;
    fn destroy(&self, world_id: WorldId) -> Result<(), WorkerError>;
    fn inspect(&self, world_id: WorldId) -> Result<WorldInspection, WorkerError>;
    fn start(&self, world_id: WorldId) -> Result<GuestAccess, WorkerError>;
    fn stop(&self, world_id: WorldId) -> Result<(), WorkerError>;
    fn disk_usage(&self, world_id: WorldId) -> Result<u64, WorkerError>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn creation_timing_has_stable_precision() {
        let mut output = Vec::new();
        super::write_creation_timing(
            &mut output,
            "configure guest access",
            std::time::Duration::from_millis(1250),
        )
        .unwrap();

        insta::assert_snapshot!(String::from_utf8(output).unwrap(), @"World creation timing: configure guest access took 1.250s\n");
    }
}
