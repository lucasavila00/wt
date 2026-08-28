extern crate self as wt_agent_tool_gateway;

mod gateway;
#[path = "bin/git-remote-wt-agent.rs"]
pub mod git_remote_command;
mod protocol;
#[path = "bin/relay.rs"]
pub mod relay_command;
mod stream;
#[path = "bin/wt-tools.rs"]
pub mod tools_command;
mod vsock;

pub use gateway::{
    wt_tools_help, ActivityRecorder, FixtureApi, Gateway, GatewayConfig, PaneObservationSnapshot,
    Provider,
};
pub use protocol::{
    valid_byobu_pane_id, valid_byobu_tmux_session, validate_pane_observations, ClientOperation,
    ClientRequest, ControlRequest, ControlResponse, Grant, PaneObservation, TransportRequest,
    TransportResponse, PROTOCOL_VERSION,
};
pub use stream::{copy_bidirectional, read_json_line, write_json_line};
pub use vsock::{VsockListener, VsockStream};
pub use wt_git_smart_protocol::{DuplexStream, GitService, WritePolicy};
pub use wt_tools::ProviderKind;

pub const VSOCK_PORT: u32 = 18017;
pub const VSOCK_PORT_ENV: &str = "WT_AGENT_TOOL_VSOCK_PORT";
pub const CONTROL_SOCKET: &str = "/run/wt-agent-tool-gateway/control.sock";
pub const RELAY_SOCKET: &str = "/run/wt-agent-tool-gateway/gateway.sock";
pub const BRANCH_PREFIX: &str = "wt/";

pub fn resolve_vsock_port(explicit: Option<u32>) -> anyhow::Result<u32> {
    match explicit {
        Some(port) => validate_vsock_port(port),
        None => Ok(vsock_port_from_env()?.unwrap_or(VSOCK_PORT)),
    }
}

pub fn start_vsock(gateway: Gateway, port: u32) -> anyhow::Result<()> {
    let listener = VsockListener::bind(u32::MAX, validate_vsock_port(port)?)
        .map_err(|error| anyhow::anyhow!("bind gateway vsock: {error}"))?;
    std::thread::Builder::new()
        .name("wt-agent-tool-gateway".to_owned())
        .spawn(move || loop {
            match listener.accept() {
                Ok(stream) => {
                    let gateway = gateway.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = gateway.handle_transport(stream) {
                            eprintln!("wt-server: agent tool request: {error:#}");
                        }
                    });
                }
                Err(error) => eprintln!("wt-server: accept agent tool request: {error}"),
            }
        })
        .map_err(|error| anyhow::anyhow!("start agent tool gateway: {error}"))?;
    Ok(())
}

pub fn vsock_port_from_env() -> anyhow::Result<Option<u32>> {
    std::env::var(VSOCK_PORT_ENV)
        .ok()
        .map(|value| {
            validate_vsock_port(
                value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("{VSOCK_PORT_ENV} must be an integer"))?,
            )
        })
        .transpose()
}

fn validate_vsock_port(port: u32) -> anyhow::Result<u32> {
    if port == 0 || port == u32::MAX {
        anyhow::bail!("agent tool vsock port must be a concrete nonzero port");
    }
    Ok(port)
}

#[derive(Clone, Debug)]
pub struct ControlClient {
    socket: std::path::PathBuf,
}

impl ControlClient {
    pub fn new(socket: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub fn request(&self, request: &ControlRequest) -> anyhow::Result<ControlResponse> {
        let mut stream = std::os::unix::net::UnixStream::connect(&self.socket)
            .map_err(|error| anyhow::anyhow!("connect to agent tool gateway: {error}"))?;
        write_json_line(&mut stream, request)?;
        read_json_line(&mut stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_vsock_ports_are_validated() {
        assert_eq!(resolve_vsock_port(Some(22017)).unwrap(), 22017);
        assert!(resolve_vsock_port(Some(0)).is_err());
        assert!(resolve_vsock_port(Some(u32::MAX)).is_err());
    }
}
