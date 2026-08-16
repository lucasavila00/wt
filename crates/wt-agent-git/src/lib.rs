mod api;
mod gateway;
mod protocol;
mod stream;
mod vsock;

pub use gateway::{FixtureApi, Gateway, GatewayConfig, Provider, ProviderKind};
pub use protocol::{
    ClientOperation, ClientRequest, ControlRequest, ControlResponse, GitService, Grant, Repository,
    TransportRequest, TransportResponse, PROTOCOL_VERSION,
};
pub use stream::{copy_bidirectional, read_json_line, write_json_line, DuplexStream};
pub use vsock::{VsockListener, VsockStream};

pub const VSOCK_PORT: u32 = 18017;
pub const CONTROL_SOCKET: &str = "/run/wt-agent-git/control.sock";
pub const BRANCH_PREFIX: &str = "wt/";

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
