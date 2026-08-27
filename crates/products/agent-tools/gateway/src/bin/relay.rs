use anyhow::{bail, Context, Result};
use clap::Parser;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use wt_agent_tool_gateway::{
    copy_bidirectional, read_json_line, resolve_vsock_port, valid_byobu_pane_id,
    valid_byobu_tmux_session, write_json_line, ClientOperation, ClientRequest, PaneObservation,
    TransportRequest, TransportResponse, VsockStream, RELAY_SOCKET,
};

const OBSERVER_INTERVAL: Duration = Duration::from_secs(2);
const OBSERVER_INTERVAL_MIN: Duration = Duration::from_millis(1500);
const OBSERVER_INTERVAL_MAX: Duration = Duration::from_millis(2500);
const FRESHNESS_INTERVAL: Duration = Duration::from_secs(15);
const CAPTURE_PANE_OPTIONS: &[&str] = &["capture-pane", "-p", "-e", "-J", "-S", "-"];

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

#[allow(dead_code)]
fn main() {
    if let Err(error) = run_from(std::env::args_os()) {
        eprintln!("wt-agent-tool-gateway-relay: {error:#}");
        std::process::exit(1);
    }
}

pub fn run_from(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
) -> Result<()> {
    let cli = Cli::parse_from(args);
    let vsock_port = resolve_vsock_port(cli.vsock_port)?;
    let token = fs::read_to_string(&cli.grant_file)
        .with_context(|| format!("read {}", cli.grant_file.display()))?;
    let token = token.trim();
    if token.is_empty() {
        bail!("gateway grant is empty");
    }
    std::thread::spawn({
        let token = token.to_owned();
        let gateway_unix = cli.gateway_unix.clone();
        move || observe_panes(token, gateway_unix, vsock_port)
    });
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
    if matches!(request.operation, ClientOperation::PaneObservations { .. }) {
        bail!("pane observations are relay-internal");
    }
    let streams_git = matches!(&request.operation, ClientOperation::Git { .. });
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

fn observe_panes(token: String, gateway_unix: Option<PathBuf>, vsock_port: u32) {
    let mut previous = BTreeMap::new();
    let mut last_sent = Instant::now() - FRESHNESS_INTERVAL;
    loop {
        match read_panes() {
            Ok(panes) => {
                let fingerprints = panes
                    .iter()
                    .map(|pane| {
                        (
                            (pane.tmux_session.clone(), pane.pane_id.clone()),
                            pane.screen_fingerprint.clone(),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                let changed = fingerprints != previous;
                if changed || last_sent.elapsed() >= FRESHNESS_INTERVAL {
                    match send_transport(
                        &token,
                        gateway_unix.as_ref(),
                        vsock_port,
                        ClientOperation::PaneObservations { panes },
                    ) {
                        Ok(response) if response.ok => {
                            previous = fingerprints;
                            last_sent = Instant::now();
                        }
                        Ok(response) => eprintln!(
                            "wt-agent-tool-gateway-relay: pane observation transport failed: {}",
                            response.error.unwrap_or_else(|| "unknown error".into())
                        ),
                        Err(error) => eprintln!(
                            "wt-agent-tool-gateway-relay: pane observation transport failed: {error:#}"
                        ),
                    }
                }
            }
            Err(error) => eprintln!("wt-agent-tool-gateway-relay: read panes: {error:#}"),
        }
        std::thread::sleep(jittered_observer_interval());
    }
}

fn jittered_observer_interval() -> Duration {
    let mut bytes = [0; 2];
    if getrandom::fill(&mut bytes).is_err() {
        return OBSERVER_INTERVAL;
    }
    observer_interval(u16::from_le_bytes(bytes))
}

fn observer_interval(random: u16) -> Duration {
    let range_millis = (OBSERVER_INTERVAL_MAX - OBSERVER_INTERVAL_MIN).as_millis();
    let jitter_millis = range_millis * u128::from(random) / u128::from(u16::MAX);
    OBSERVER_INTERVAL_MIN + Duration::from_millis(jitter_millis as u64)
}

fn read_panes() -> Result<Vec<PaneObservation>> {
    let output = Command::new("/usr/bin/tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}\t#{pane_id}\t#{pane_current_command}",
        ])
        .output()
        .context("list Byobu panes")?;
    if !output.status.success() {
        bail!("list Byobu panes failed: {}", escaped(&output.stderr));
    }
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(parse_codex_target)
        .map(|(tmux_session, pane_id)| capture_pane(&tmux_session, &pane_id))
        .collect()
}

fn parse_codex_target(line: &[u8]) -> Option<(String, String)> {
    let (tmux_session, rest) = std::str::from_utf8(line).ok()?.split_once('\t')?;
    let (pane_id, command) = rest.split_once('\t')?;
    (valid_byobu_tmux_session(tmux_session) && valid_byobu_pane_id(pane_id) && command == "codex")
        .then(|| (tmux_session.to_owned(), pane_id.to_owned()))
}

fn capture_pane(tmux_session: &str, pane_id: &str) -> Result<PaneObservation> {
    let output = Command::new("/usr/bin/tmux")
        .args(CAPTURE_PANE_OPTIONS)
        .args(["-t", pane_id])
        .output()
        .with_context(|| format!("capture Byobu pane {pane_id}"))?;
    if !output.status.success() {
        bail!(
            "capture Byobu pane {pane_id} failed: {}",
            escaped(&output.stderr)
        );
    }
    Ok(PaneObservation {
        tmux_session: tmux_session.to_owned(),
        pane_id: pane_id.to_owned(),
        screen_fingerprint: screen_fingerprint(&output.stdout),
    })
}

fn screen_fingerprint(captured: &[u8]) -> String {
    // `-J` turns terminal-width wrapping back into logical lines. The remaining
    // trailing blank rows are viewport padding, so they must not mark a paused
    // Codex pane as active when its playback PTY changes size.
    format!("{:x}", Sha256::digest(captured.trim_ascii_end()))
}

fn send_transport(
    token: &str,
    gateway_unix: Option<&PathBuf>,
    vsock_port: u32,
    operation: ClientOperation,
) -> Result<TransportResponse> {
    let request = TransportRequest {
        protocol_version: wt_agent_tool_gateway::PROTOCOL_VERSION,
        token: token.into(),
        operation,
    };
    if let Some(path) = gateway_unix {
        let mut stream = UnixStream::connect(path)?;
        write_json_line(&mut stream, &request)?;
        return read_json_line(&mut stream).context("read pane observation response");
    }
    let mut stream = VsockStream::connect(2, vsock_port)?;
    write_json_line(&mut stream, &request)?;
    read_json_line(&mut stream).context("read pane observation response")
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
    fn accepts_only_codex_panes_in_wt_byobu() {
        assert_eq!(
            parse_codex_target(b"wt-host\t%1\tcodex"),
            Some(("wt-host".into(), "%1".into()))
        );
        assert_eq!(parse_codex_target(b"wt-host\t%1\tbash"), None);
        assert_eq!(parse_codex_target(b"other\t%1\tcodex"), None);
        assert_eq!(parse_codex_target(b"wt-host\t%bad\tcodex"), None);
    }

    #[test]
    fn observer_interval_has_twenty_five_percent_jitter() {
        assert_eq!(observer_interval(0), Duration::from_millis(1500));
        assert_eq!(observer_interval(u16::MAX), Duration::from_millis(2500));
    }

    #[test]
    fn pane_fingerprint_ignores_viewport_padding() {
        let compact = b"Codex is waiting for input\n\n";
        let expanded = b"Codex is waiting for input\n\n\n\n\n";

        assert_eq!(screen_fingerprint(compact), screen_fingerprint(expanded));
        assert_ne!(
            screen_fingerprint(compact),
            screen_fingerprint(b"Codex is working\n")
        );
    }

    #[test]
    fn pane_capture_joins_wrapped_history() {
        assert_eq!(
            CAPTURE_PANE_OPTIONS,
            ["capture-pane", "-p", "-e", "-J", "-S", "-"]
        );
    }
}
