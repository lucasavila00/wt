use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use wt_agent_tool_gateway::{
    copy_bidirectional, read_json_line, resolve_vsock_port, valid_codex_pane_id,
    valid_codex_tmux_session, write_json_line, ClientOperation, ClientRequest,
    CodexSessionEventKind, TransportRequest, TransportResponse, VsockStream,
    CODEX_SESSION_PANE_OPTION, RELAY_SOCKET,
};

const TRACKER_STATE_FILE: &str = ".local/state/wt/codex-git-tracker.json";
const GIT_TIMEOUT: Duration = Duration::from_millis(500);
const TRACKER_INTERVAL: Duration = Duration::from_secs(2);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

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
    #[arg(long)]
    tracker_state_file: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Registration {
    cwd: String,
    tmux_session: String,
    pane_id: String,
    pane_generation: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct TrackerState {
    sessions: BTreeMap<uuid::Uuid, Registration>,
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
    let tracker_state_file = cli.tracker_state_file.unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(TRACKER_STATE_FILE)
    });
    let tracker = Arc::new(Mutex::new(load_tracker(&tracker_state_file)?));
    let (tracker_wake, tracker_events) = mpsc::channel();
    std::thread::spawn({
        let token = token.to_owned();
        let gateway_unix = cli.gateway_unix.clone();
        let tracker = Arc::clone(&tracker);
        let tracker_state_file = tracker_state_file.clone();
        move || {
            track_git_context(
                tracker,
                tracker_events,
                token,
                gateway_unix,
                vsock_port,
                tracker_state_file,
            )
        }
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
                let tracker = Arc::clone(&tracker);
                let tracker_state_file = tracker_state_file.clone();
                let tracker_wake = tracker_wake.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle(
                        stream,
                        &token,
                        gateway_unix,
                        vsock_port,
                        &tracker,
                        &tracker_state_file,
                        &tracker_wake,
                    ) {
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
    tracker: &Arc<Mutex<TrackerState>>,
    tracker_state_file: &PathBuf,
    tracker_wake: &mpsc::Sender<()>,
) -> Result<()> {
    let request: ClientRequest = read_json_line(&mut client)?;
    validate_codex_target(&request.operation)?;
    let codex_event = match &request.operation {
        ClientOperation::CodexSession { event } => Some(event.clone()),
        _ => None,
    };
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
        if response.ok && response.message.as_deref() == Some("accepted") {
            if let Some(event) = &codex_event {
                update_codex_marker(event)?;
                update_registration(event, tracker, tracker_state_file)?;
                let _ = tracker_wake.send(());
            }
        }
        write_json_line(&mut client, &response)?;
        if response.ok && streams_git {
            copy_bidirectional(client, gateway)?;
        }
    } else {
        let mut gateway = VsockStream::connect(2, vsock_port).context("connect to host gateway")?;
        write_json_line(&mut gateway, &request)?;
        let response: TransportResponse = read_json_line(&mut gateway)?;
        if response.ok && response.message.as_deref() == Some("accepted") {
            if let Some(event) = &codex_event {
                update_codex_marker(event)?;
                update_registration(event, tracker, tracker_state_file)?;
                let _ = tracker_wake.send(());
            }
        }
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
    Ok(())
}

fn update_codex_marker(event: &wt_agent_tool_gateway::CodexSessionEvent) -> Result<()> {
    if event.kind == CodexSessionEventKind::SessionEnd {
        let (condition, clear) = clear_marker_command(event);
        let status = Command::new("/usr/bin/tmux")
            .args(["if-shell", "-F", "-t", &event.pane_id, &condition, &clear])
            .status()
            .context("clear matching Codex session pane marker")?;
        if !status.success() {
            bail!("could not clear matching Codex session pane marker");
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

fn update_registration(
    event: &wt_agent_tool_gateway::CodexSessionEvent,
    tracker: &Arc<Mutex<TrackerState>>,
    state_file: &PathBuf,
) -> Result<()> {
    let mut tracker = tracker
        .lock()
        .map_err(|_| anyhow::anyhow!("Codex Git tracker lock poisoned"))?;
    if event.kind == CodexSessionEventKind::SessionEnd {
        tracker.sessions.remove(&event.session_id);
    } else {
        tracker.sessions.insert(
            event.session_id,
            Registration {
                cwd: event.cwd.clone(),
                tmux_session: event.tmux_session.clone(),
                pane_id: event.pane_id.clone(),
                pane_generation: event.pane_generation,
            },
        );
    }
    save_tracker(state_file, &tracker)
}

fn load_tracker(path: &PathBuf) -> Result<TrackerState> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("decode Codex Git tracker state"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(TrackerState::default()),
        Err(error) => Err(error).context("read Codex Git tracker state"),
    }
}

fn save_tracker(path: &PathBuf, tracker: &TrackerState) -> Result<()> {
    let parent = path
        .parent()
        .context("Codex Git tracker state has no parent directory")?;
    fs::create_dir_all(parent).context("create Codex Git tracker state directory")?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, serde_json::to_vec(tracker)?)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(temporary, path).context("replace Codex Git tracker state")
}

fn track_git_context(
    tracker: Arc<Mutex<TrackerState>>,
    wake: mpsc::Receiver<()>,
    token: String,
    gateway_unix: Option<PathBuf>,
    vsock_port: u32,
    state_file: PathBuf,
) {
    let mut sent = BTreeMap::<uuid::Uuid, (GitContext, Instant)>::new();
    loop {
        let registrations = match tracker.lock() {
            Ok(tracker) => tracker.sessions.clone(),
            Err(_) => {
                eprintln!("wt-agent-tool-gateway-relay: Codex Git tracker lock poisoned");
                return;
            }
        };
        for (session_id, registration) in registrations {
            if !marker_matches(session_id, &registration) {
                if let Ok(mut state) = tracker.lock() {
                    state.sessions.remove(&session_id);
                    if let Err(error) = save_tracker(&state_file, &state) {
                        eprintln!("wt-agent-tool-gateway-relay: remove stale Git tracker registration: {error:#}");
                    }
                }
                continue;
            }
            let context = git_context(&registration.cwd);
            let unchanged = sent.get(&session_id).is_some_and(|(previous, at)| {
                previous == &context && at.elapsed() < HEARTBEAT_INTERVAL
            });
            if unchanged {
                continue;
            }
            let request = wt_agent_tool_gateway::ClientOperation::CodexGitContext {
                context: wt_agent_tool_gateway::CodexGitContext {
                    session_id,
                    cwd: registration.cwd.clone(),
                    tmux_session: registration.tmux_session.clone(),
                    pane_id: registration.pane_id.clone(),
                    pane_generation: registration.pane_generation,
                    repository_root: context.repository_root.clone(),
                    repository_url: context.repository_url.clone(),
                    git_branch: context.git_branch.clone(),
                    error: context.error.clone(),
                },
            };
            match send_transport(&token, gateway_unix.as_ref(), vsock_port, request) {
                Ok(response) if response.ok => {
                    sent.insert(session_id, (context, Instant::now()));
                }
                Ok(response) => eprintln!(
                    "wt-agent-tool-gateway-relay: Git context transport failed for {session_id} {}: {}",
                    registration.pane_id,
                    response.error.unwrap_or_else(|| "unknown error".into())
                ),
                Err(error) => eprintln!(
                    "wt-agent-tool-gateway-relay: Git context transport failed for {session_id} {}: {error:#}",
                    registration.pane_id,
                ),
            }
        }
        let _ = wake.recv_timeout(TRACKER_INTERVAL);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GitContext {
    repository_root: Option<String>,
    repository_url: Option<String>,
    git_branch: Option<String>,
    error: Option<String>,
}

fn git_context(cwd: &str) -> GitContext {
    let root = match run_git(cwd, &["rev-parse", "--show-toplevel"]) {
        Ok(Some(root)) => root,
        Ok(None) => {
            return GitContext {
                repository_root: None,
                repository_url: None,
                git_branch: None,
                error: None,
            }
        }
        Err(error) => return failed_git_context(error),
    };
    let repository_url = match run_git(cwd, &["config", "--get", "remote.origin.url"]) {
        Ok(value) => value,
        Err(error) => return failed_git_context(error),
    };
    let git_branch = match run_git(cwd, &["branch", "--show-current"]) {
        Ok(value) => value,
        Err(error) => return failed_git_context(error),
    };
    GitContext {
        repository_root: Some(root),
        repository_url,
        git_branch,
        error: None,
    }
}

fn failed_git_context(error: anyhow::Error) -> GitContext {
    GitContext {
        repository_root: None,
        repository_url: None,
        git_branch: None,
        error: Some(
            error
                .to_string()
                .chars()
                .filter(|character| !character.is_control())
                .take(512)
                .collect(),
        ),
    }
}

fn run_git(cwd: &str, arguments: &[&str]) -> Result<Option<String>> {
    let mut child = Command::new("/usr/bin/git")
        .args(["-C", cwd])
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start Git state command")?;
    let deadline = Instant::now() + GIT_TIMEOUT;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Git state command timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .context("Git state stdout unavailable")?
        .take(16 * 1024)
        .read_to_end(&mut output)?;
    let status = child.wait()?;
    if !status.success() {
        return Ok(None);
    }
    let value = String::from_utf8(output).context("Git state output was not UTF-8")?;
    Ok(Some(value.trim().to_owned()).filter(|value| !value.is_empty()))
}

fn marker_matches(session_id: uuid::Uuid, registration: &Registration) -> bool {
    let output = Command::new("/usr/bin/tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &registration.pane_id,
            "#{session_name}:#{pane_id}:#{@wt_codex_session_id}",
        ])
        .output();
    output.is_ok_and(|output| {
        output.status.success()
            && output.stdout
                == format!(
                    "{}:{}:{}\n",
                    registration.tmux_session, registration.pane_id, session_id
                )
                .as_bytes()
    })
}

fn send_transport(
    token: &str,
    gateway_unix: Option<&PathBuf>,
    vsock_port: u32,
    operation: wt_agent_tool_gateway::ClientOperation,
) -> Result<TransportResponse> {
    let request = TransportRequest {
        protocol_version: wt_agent_tool_gateway::PROTOCOL_VERSION,
        token: token.into(),
        operation,
    };
    if let Some(path) = gateway_unix {
        let mut stream = UnixStream::connect(path)?;
        write_json_line(&mut stream, &request)?;
        return read_json_line(&mut stream).context("read Git context response");
    }
    let mut stream = VsockStream::connect(2, vsock_port)?;
    write_json_line(&mut stream, &request)?;
    read_json_line(&mut stream).context("read Git context response")
}

fn clear_marker_command(event: &wt_agent_tool_gateway::CodexSessionEvent) -> (String, String) {
    (
        format!(
            "#{{==:#{{{CODEX_SESSION_PANE_OPTION}}},{}}}",
            event.session_id
        ),
        format!(
            "set-option -p -u -t {} {CODEX_SESSION_PANE_OPTION}",
            event.pane_id
        ),
    )
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
    fn clears_the_marker_with_one_conditional_tmux_command() {
        let event = wt_agent_tool_gateway::CodexSessionEvent {
            session_id: uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            cwd: "/home/wt/project".into(),
            repository_root: None,
            repository_url: None,
            git_branch: None,
            tmux_session: "wt-host".into(),
            pane_id: "%1".into(),
            kind: CodexSessionEventKind::SessionEnd,
            pane_generation: 1,
            pane_sequence: 1,
            session_start_source: None,
        };

        assert_eq!(
            clear_marker_command(&event),
            (
                "#{==:#{@wt_codex_session_id},123e4567-e89b-12d3-a456-426614174000}".into(),
                "set-option -p -u -t %1 @wt_codex_session_id".into(),
            )
        );
    }

    #[test]
    fn discovers_normal_detached_and_non_repository_contexts() {
        let temp = tempfile::tempdir().unwrap();
        let status = Command::new("/usr/bin/git")
            .args(["init", "-b", "wt/tracker"])
            .arg(temp.path())
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(temp.path())
            .args(["remote", "add", "origin", "git@github.com:acme/project.git"])
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(temp.path())
            .args([
                "-c",
                "user.name=WT",
                "-c",
                "user.email=wt@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ])
            .status()
            .unwrap();
        assert!(status.success());

        let context = git_context(temp.path().to_str().unwrap());
        assert_eq!(context.repository_root.as_deref(), temp.path().to_str());
        assert_eq!(
            context.repository_url.as_deref(),
            Some("git@github.com:acme/project.git")
        );
        assert_eq!(context.git_branch.as_deref(), Some("wt/tracker"));

        let status = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(temp.path())
            .args(["checkout", "--detach"])
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(git_context(temp.path().to_str().unwrap()).git_branch, None);

        let non_repository = tempfile::tempdir().unwrap();
        assert_eq!(
            git_context(non_repository.path().to_str().unwrap()),
            GitContext {
                repository_root: None,
                repository_url: None,
                git_branch: None,
                error: None,
            }
        );
    }
}
