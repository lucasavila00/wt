use super::action_queue::ActionId;
use super::control::PaneCardIdentity;
use super::model::ShellModel;
use super::session::SessionSet;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct FocusResult {
    pub(super) action_id: ActionId,
    pub(super) target: PaneCardIdentity,
    pub(super) control_path: PathBuf,
    pub(super) result: Result<(), String>,
}

pub(super) struct FocusWorker {
    sender: Sender<FocusResult>,
    receiver: Receiver<FocusResult>,
}

impl Default for FocusWorker {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }
}

impl FocusWorker {
    pub(super) fn start(
        &self,
        action_id: ActionId,
        sessions: &SessionSet,
        model: &ShellModel,
        target: PaneCardIdentity,
    ) -> bool {
        let PaneCardIdentity::Observation {
            tmux_session,
            pane_id,
            ..
        } = &target
        else {
            return false;
        };
        let tmux_session = tmux_session.clone();
        let pane_id = pane_id.clone();
        let Some((index, alias)) = model.pane_route(&target) else {
            return false;
        };
        if !sessions.is_open(index) {
            return false;
        }
        let alias = alias.to_owned();
        let control_path = sessions.control_path(index).to_owned();
        let result_control_path = control_path.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = focus(&alias, &control_path, &tmux_session, &pane_id)
                .map_err(|error| error.to_string());
            let _ = sender.send(FocusResult {
                action_id,
                target,
                control_path: result_control_path,
                result,
            });
        });
        true
    }

    pub(super) fn try_recv(&self) -> Option<FocusResult> {
        self.receiver.try_recv().ok()
    }
}

fn focus(
    alias: &str,
    control_path: &Path,
    tmux_session: &str,
    pane_id: &str,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    wait_for_control_master(alias, control_path, deadline)?;
    let mut command = Command::new("ssh");
    command
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "BatchMode=yes",
            "-o",
            "ProxyCommand=/bin/false",
            "-S",
        ])
        .arg(control_path)
        .args([
            "--",
            alias,
            "/usr/local/bin/wtg",
            "codex",
            "focus-pane",
            tmux_session,
            pane_id,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let child = command
        .spawn()
        .map_err(|error| anyhow::anyhow!("start Codex pane focus helper: {error}"))?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let output = wt_client::transport::wait_with_output_timeout(child, remaining)
        .map_err(|error| anyhow::anyhow!("wait for Codex pane focus helper: {error}"))?;
    let expected = format!("{tmux_session}:{pane_id}\n");
    if !output.status.success() || output.stdout != expected.as_bytes() {
        anyhow::bail!(
            "focus helper failed: status {}; expected stdout {}; actual stdout {}; stderr {}",
            output.status,
            bounded_escaped(expected.as_bytes()),
            bounded_escaped(&output.stdout),
            bounded_escaped(&output.stderr)
        );
    }
    Ok(())
}

fn wait_for_control_master(
    alias: &str,
    control_path: &Path,
    deadline: Instant,
) -> anyhow::Result<()> {
    while Instant::now() < deadline {
        if control_path.exists() {
            let status = Command::new("ssh")
                .args(["-S"])
                .arg(control_path)
                .args(["-o", "ProxyCommand=/bin/false", "-O", "check", "--", alias])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|error| anyhow::anyhow!("check shell SSH connection: {error}"))?;
            if status.success() {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(16));
    }
    anyhow::bail!("shell SSH connection was not ready before the focus deadline")
}

fn bounded_escaped(bytes: &[u8]) -> String {
    let mut escaped = String::new();
    for character in String::from_utf8_lossy(bytes).chars() {
        for escaped_character in character.escape_default() {
            if escaped.len() == 256 {
                escaped.push('…');
                return escaped;
            }
            escaped.push(escaped_character);
        }
    }
    escaped
}
