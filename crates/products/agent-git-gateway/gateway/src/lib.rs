mod gateway;
mod protocol;
mod stream;
mod vsock;

pub use gateway::{FixtureApi, Gateway, GatewayConfig, Provider};
pub use wt_git_hosting::ProviderKind;
pub use protocol::{
    ClientOperation, ClientRequest, ControlRequest, ControlResponse, Grant, Repository,
    TransportRequest, TransportResponse, PROTOCOL_VERSION,
};
pub use stream::{copy_bidirectional, read_json_line, write_json_line};
pub use vsock::{VsockListener, VsockStream};
pub use wt_git_smart_protocol::{DuplexStream, GitService, WritePolicy};

pub const VSOCK_PORT: u32 = 18017;
pub const VSOCK_PORT_ENV: &str = "WT_AGENT_GIT_VSOCK_PORT";
pub const CONTROL_SOCKET: &str = "/run/wt-agent-git-gateway/control.sock";
pub const BRANCH_PREFIX: &str = "wt/";

pub fn resolve_vsock_port(explicit: Option<u32>) -> anyhow::Result<u32> {
    match explicit {
        Some(port) => validate_vsock_port(port),
        None => Ok(vsock_port_from_env()?.unwrap_or(VSOCK_PORT)),
    }
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
        anyhow::bail!("agent Git vsock port must be a concrete nonzero port");
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
            .map_err(|error| anyhow::anyhow!("connect to agent Git gateway: {error}"))?;
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
