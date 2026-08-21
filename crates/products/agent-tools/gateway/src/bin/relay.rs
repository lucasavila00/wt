use anyhow::{bail, Context, Result};
use clap::Parser;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Command;
use wt_agent_tool_gateway::{
    copy_bidirectional, read_json_line, resolve_vsock_port, valid_codex_pane_id,
    valid_codex_tmux_session, write_json_line, ClientOperation, ClientRequest,
    CodexSessionEventKind, TransportRequest, TransportResponse, VsockStream,
    CODEX_SESSION_PANE_OPTION, RELAY_SOCKET,
};

#[derive(Debug, Parser)]
#[command(name = "wt-agent-tool-gateway-relay")]
struct Cli {
    #[arg(long, default_value = RELAY_SOCKET)]
    socket: PathBuf,
    #[arg(long, default_value = "/var/lib/wt-agent-tool-gateway/grant")]
    grant_file: PathBuf,
    #[arg(long)]
    gateway_unix: Option<PathBuf>,
    #[arg(long)]
    vsock_port: Option<u32>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-agent-tool-gateway-relay: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let vsock_port = resolve_vsock_port(cli.vsock_port)?;
    let token = fs::read_to_string(&cli.grant_file)
        .with_context(|| format!("read {}", cli.grant_file.display()))?;
    let token = token.trim();
    if token.is_empty() {
        bail!("gateway grant is empty");
    }
    if let Some(parent) = cli.socket.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    match fs::remove_file(&cli.socket) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("remove stale {}", cli.socket.display()))
        }
    }
    let listener = UnixListener::bind(&cli.socket)
        .with_context(|| format!("bind {}", cli.socket.display()))?;
    fs::set_permissions(&cli.socket, fs::Permissions::from_mode(0o666))
        .with_context(|| format!("set permissions on {}", cli.socket.display()))?;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let token = token.to_owned();
                let gateway_unix = cli.gateway_unix.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream, &token, gateway_unix, vsock_port) {
                        eprintln!("wt-agent-tool-gateway-relay: request: {error:#}");
                    }
                });
            }
            Err(error) => eprintln!("wt-agent-tool-gateway-relay: accept: {error}"),
        }
    }
    Ok(())
}

fn handle(
    mut client: UnixStream,
    token: &str,
    gateway_unix: Option<PathBuf>,
    vsock_port: u32,
) -> Result<()> {
    let request: ClientRequest = read_json_line(&mut client)?;
    validate_codex_target(&request.operation)?;
    let streams_git = matches!(
        &request.operation,
        wt_agent_tool_gateway::ClientOperation::Git { .. }
    );
    let request = TransportRequest {
        protocol_version: request.protocol_version,
        token: token.to_owned(),
        operation: request.operation,
    };
    if let Some(path) = gateway_unix {
        let mut gateway = UnixStream::connect(&path)
            .with_context(|| format!("connect to gateway {}", path.display()))?;
        write_json_line(&mut gateway, &request)?;
        let response: TransportResponse = read_json_line(&mut gateway)?;
        write_json_line(&mut client, &response)?;
        if response.ok && streams_git {
            copy_bidirectional(client, gateway)?;
        }
    } else {
        let mut gateway = VsockStream::connect(2, vsock_port).context("connect to host gateway")?;
        write_json_line(&mut gateway, &request)?;
        let response: TransportResponse = read_json_line(&mut gateway)?;
        write_json_line(&mut client, &response)?;
        if response.ok && streams_git {
            copy_bidirectional(client, gateway)?;
        }
    }
    Ok(())
}

fn validate_codex_target(operation: &ClientOperation) -> Result<()> {
    let ClientOperation::CodexSession { event } = operation else {
        return Ok(());
    };
    if !valid_codex_tmux_session(&event.tmux_session) || !valid_codex_pane_id(&event.pane_id) {
        bail!("invalid Codex Byobu target");
    }
    let output = Command::new("/usr/bin/tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &event.pane_id,
            "#{session_name}:#{pane_id}",
        ])
        .output()
        .context("validate Codex Byobu target")?;
    let expected = format!("{}:{}\n", event.tmux_session, event.pane_id);
    if !output.status.success() || output.stdout != expected.as_bytes() {
        bail!(
            "Codex Byobu target validation failed: status {}; expected stdout {}; actual stdout {}; stderr {}",
            output.status,
            escaped(expected.as_bytes()),
            escaped(&output.stdout),
            escaped(&output.stderr)
        );
    }
    update_codex_marker(event)?;
    Ok(())
}

fn update_codex_marker(event: &wt_agent_tool_gateway::CodexSessionEvent) -> Result<()> {
    if event.kind == CodexSessionEventKind::SessionEnd {
        let output = Command::new("/usr/bin/tmux")
            .args([
                "show-options",
                "-p",
                "-v",
                "-t",
                &event.pane_id,
                CODEX_SESSION_PANE_OPTION,
            ])
            .output()
            .context("read Codex session pane marker")?;
        if !output.status.success() {
            return Ok(());
        }
        if marker_matches(&output.stdout, event.session_id) {
            let status = Command::new("/usr/bin/tmux")
                .args([
                    "set-option",
                    "-p",
                    "-u",
                    "-t",
                    &event.pane_id,
                    CODEX_SESSION_PANE_OPTION,
                ])
                .status()
                .context("clear Codex session pane marker")?;
            if !status.success() {
                bail!("could not clear Codex session pane marker");
            }
        }
        return Ok(());
    }

    let status = Command::new("/usr/bin/tmux")
        .args([
            "set-option",
            "-p",
            "-t",
            &event.pane_id,
            CODEX_SESSION_PANE_OPTION,
            &event.session_id.to_string(),
        ])
        .status()
        .context("write Codex session pane marker")?;
    if !status.success() {
        bail!("could not write Codex session pane marker");
    }
    Ok(())
}

fn marker_matches(output: &[u8], session_id: uuid::Uuid) -> bool {
    output == format!("{session_id}\n").as_bytes()
}

fn escaped(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(char::escape_default)
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_only_the_exact_session_marker() {
        let session_id = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        assert!(marker_matches(
            b"123e4567-e89b-12d3-a456-426614174000\n",
            session_id
        ));
        assert!(!marker_matches(
            b"223e4567-e89b-12d3-a456-426614174000\n",
            session_id
        ));
        assert!(!marker_matches(
            b"123e4567-e89b-12d3-a456-426614174000",
            session_id
        ));
    }
}
