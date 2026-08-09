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
    let mut args = std::env::args();
    let _program = args.next();
    if args.next().as_deref() != Some("cli") {
        bail!("internal usage: wt-agent-git-client cli [ARG...]");
    }
    let args = args.collect();
    let branch = current_branch();
    let socket = test_socket();
    let mut relay = UnixStream::connect(&socket).context("connect to WT Git relay")?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::Cli { args, branch },
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
    let mut input_relay = relay.try_clone().context("clone WT Git relay stream")?;
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut input_relay);
        let _ = input_relay.flush();
        let _ = input_relay.shutdown(std::net::Shutdown::Write);
    });
    std::io::copy(&mut relay, &mut std::io::stdout()).context("read gateway output")?;
    Ok(())
}

fn current_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .ok()?;
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
