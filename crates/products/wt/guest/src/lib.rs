use std::io::Write;
use std::time::Duration;
use wt_libvirt_kvm::WorkerError;
use wt_world::WindowId;
use wt_world::WorldId;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLaunch {
    pub window_id: WindowId,
    pub argv: Vec<String>,
    pub cwd: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowStarted {
    pub tmux_window_id: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowObservation {
    pub tmux_window_id: String,
    pub state: wt_control_protocol::WindowState,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    pub output_offset: u64,
    pub output: Vec<WindowOutputChunk>,
    pub screen: String,
    pub screen_observed_at_unix_ms: i64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowOutputChunk {
    pub channel: wt_control_protocol::WindowOutputChannel,
    pub data: Vec<u8>,
}

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

    fn start_window(
        &self,
        world_id: WorldId,
        launch: &WindowLaunch,
    ) -> Result<WindowStarted, WorkerError> {
        let _ = (world_id, launch);
        Err(WorkerError::new("managed windows are not supported"))
    }

    fn observe_window(
        &self,
        world_id: WorldId,
        window_id: WindowId,
        output_offset: u64,
    ) -> Result<WindowObservation, WorkerError> {
        let _ = (world_id, window_id, output_offset);
        Err(WorkerError::new("managed windows are not supported"))
    }

    fn send_window_input(
        &self,
        world_id: WorldId,
        window_id: WindowId,
        sequence_id: u64,
        data: &[u8],
    ) -> Result<(), WorkerError> {
        let _ = (world_id, window_id, sequence_id, data);
        Err(WorkerError::new("managed windows are not supported"))
    }

    fn stop_window(&self, world_id: WorldId, window_id: WindowId) -> Result<(), WorkerError> {
        let _ = (world_id, window_id);
        Err(WorkerError::new("managed windows are not supported"))
    }

    fn delete_window(&self, world_id: WorldId, window_id: WindowId) -> Result<(), WorkerError> {
        let _ = (world_id, window_id);
        Err(WorkerError::new("managed windows are not supported"))
    }

    fn stop_world_windows(&self, world_id: WorldId) -> Result<(), WorkerError> {
        let _ = world_id;
        Err(WorkerError::new("managed windows are not supported"))
    }
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
