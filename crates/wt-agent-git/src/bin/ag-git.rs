use anyhow::{bail, Context, Result};
use std::io::Write;
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
    let args = std::env::args().skip(1).collect();
    let (branch, head) = current_checkout();
    let socket = test_socket();
    let mut relay = UnixStream::connect(&socket).with_context(|| {
        format!(
            "cannot reach the WT Git relay at {socket}; this command only works inside a running WT environment"
        )
    })?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::Cli { args, branch, head },
        },
    )
    .context("send command to the WT Git relay")?;
    let response: TransportResponse = read_json_line(&mut relay)
        .context("read the WT Git gateway response; the relay or gateway may have stopped")?;
    if !response.ok {
        bail!(
            "{}",
            response
                .error
                .as_deref()
                .unwrap_or("gateway rejected command")
        );
    }
    if let Some(message) = response.message {
        std::io::stdout()
            .write_all(message.as_bytes())
            .context("write gateway output")?;
    }
    Ok(())
}

fn current_checkout() -> (Option<String>, Option<String>) {
    let branch = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let head = git_output(&["rev-parse", "--verify", "HEAD"]);
    (branch, head)
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
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
