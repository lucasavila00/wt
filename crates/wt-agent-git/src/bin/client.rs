use anyhow::{bail, Context, Result};
use std::os::unix::net::UnixStream;
use wt_agent_git::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, TransportResponse,
    PROTOCOL_VERSION,
};

const SOCKET: &str = "/run/wt-agent-git/gateway.sock";

fn main() {
    if let Err(error) = run() {
        eprintln!("ag-git: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args();
    let _program = args.next();
    if args.next().as_deref() != Some("cli") {
        bail!("internal usage: wt-agent-git-client cli [ARG...]");
    }
    let args = args.collect();
    let socket = test_socket();
    let mut relay = UnixStream::connect(&socket).context("connect to WT Git relay")?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::Cli { args },
        },
    )?;
    let response: TransportResponse = read_json_line(&mut relay)?;
    if !response.ok {
        bail!(
            "{}",
            response
                .error
                .as_deref()
                .unwrap_or("gateway rejected command")
        );
    }
    std::io::copy(&mut relay, &mut std::io::stdout()).context("read gateway output")?;
    Ok(())
}

fn test_socket() -> String {
    if cfg!(debug_assertions) {
        std::env::var("WT_AGENT_GIT_TEST_SOCKET")
            .ok()
            .unwrap_or_else(|| SOCKET.to_owned())
    } else {
        SOCKET.to_owned()
    }
}
