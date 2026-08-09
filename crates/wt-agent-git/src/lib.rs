mod gateway;
mod protocol;
mod stream;
mod vsock;

pub use gateway::{Gateway, GatewayConfig, Provider};
pub use protocol::{
    ClientOperation, ClientRequest, ControlRequest, ControlResponse, GitService, Grant,
    TransportRequest, TransportResponse, PROTOCOL_VERSION,
};
pub use stream::{copy_bidirectional, read_json_line, write_json_line, DuplexStream};
pub use vsock::{VsockListener, VsockStream};

pub const VSOCK_PORT: u32 = 18017;
