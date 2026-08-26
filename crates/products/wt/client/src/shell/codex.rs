use super::control::{
    CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget, GitContextHealth,
};
use super::model::ShellModel;
pub(super) use super::model::ShellWorld;
use super::session::SessionSet;
use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;
use wt_client::config::ClientConfig;
use wt_client::inventory::ContextWorld;
use wt_control_protocol::{
    ByobuTarget, CodexSession, CodexSessionObservation, CodexSessionState, PaneObservation, World,
};

#[derive(Debug)]
pub(super) enum CodexContextSnapshot {
    Panes {
        context: String,
        panes: Vec<PaneObservation>,
    },
    Failure {
        message: String,
    },
}

pub(super) struct CodexCards {
    pub(super) cards: Vec<CodexCard>,
    pub(super) failures: Vec<String>,
}

impl ShellWorld {
    pub(super) fn from_inventory(item: &ContextWorld) -> Self {
        let mut world = Self::from_world(&item.context, &item.world);
        world.resources =
            wt_client::inventory::format_resources(&item.world, item.disk_usage_bytes);
        world.detail = wt_client::inventory::format_detail(item);
        world
    }

    pub(super) fn from_world(context: &str, world: &World) -> Self {
        let qualified_name = format!("{context}.{}", world.name);
        let control_alias = format!("{qualified_name}-direct");
        Self {
            identity: super::model::WorldIdentity {
                context: context.into(),
                world_id: world.world_id,
            },
            name: qualified_name,
            world_name: world.name.clone(),
            control_alias,
            status: world.status,
            resources: wt_client::inventory::format_resources(world, None),
            detail: world.last_error.as_deref().unwrap_or("-").into(),
        }
    }

    #[cfg(test)]
    pub(super) fn test(name: &str, index: u128) -> Self {
        let (context, world_name) = name.split_once('.').unwrap_or(("local", name));
        Self {
            identity: super::model::WorldIdentity {
                context: context.into(),
                world_id: Uuid::from_u128(index).into(),
            },
            name: name.into(),
            world_name: wt_control_protocol::WorldName::parse(world_name).unwrap(),
            control_alias: format!("{name}-direct"),
            status: wt_control_protocol::WorldStatus::Running,
            resources: "2 CPU · 4G · 1G/32G disk".into(),
            detail: "-".into(),
        }
    }
}

pub(super) fn load_snapshots(
    config: &ClientConfig,
    cancelled: &AtomicBool,
) -> Vec<CodexContextSnapshot> {
    config
        .contexts
        .iter()
        .take_while(|_| !cancelled.load(Ordering::Relaxed))
        .map(|context| {
            match wt_client::transport::call_pane_observations_with_timeout_until(
                context,
                super::CONTEXT_REQUEST_TIMEOUT,
                cancelled,
            ) {
                Ok(panes) => CodexContextSnapshot::Panes {
                    context: context.name.clone(),
                    panes,
                },
                Err(error) => CodexContextSnapshot::Failure {
                    message: error
                        .to_string()
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                },
            }
        })
        .collect()
}

pub(super) fn cards(snapshots: Vec<CodexContextSnapshot>, worlds: &[ShellWorld]) -> CodexCards {
    let mut cards = Vec::new();
    let mut failures = Vec::new();
    for snapshot in snapshots {
        match snapshot {
            CodexContextSnapshot::Panes { context, panes } => match pane_sessions(panes) {
                Ok(sessions) => match validate_context(&context, sessions, worlds) {
                    Ok(mut context_cards) => cards.append(&mut context_cards),
                    Err(message) => cards.push(CodexCard::context_error(&context, message)),
                },
                Err(message) => cards.push(CodexCard::context_error(&context, message)),
            },
            CodexContextSnapshot::Failure { message } => failures.push(message),
        }
    }
    cards.sort_by(|left, right| {
        left.sort_rank()
            .cmp(&right.sort_rank())
            .then_with(|| Reverse(left.timestamp()).cmp(&Reverse(right.timestamp())))
            .then_with(|| left.identity.cmp(&right.identity))
    });
    CodexCards { cards, failures }
}

fn pane_sessions(panes: Vec<PaneObservation>) -> Result<Vec<CodexSession>, String> {
    panes
        .into_iter()
        .map(|pane| {
            let pane_number = pane
                .pane_id
                .strip_prefix('%')
                .and_then(|value| value.parse::<u128>().ok())
                .ok_or_else(|| format!("invalid pane identifier {}", pane.pane_id))?;
            let session_id = Uuid::from_u128(pane.world_id.as_uuid().as_u128() ^ pane_number);
            let active = pane
                .observed_at_unix_ms
                .saturating_sub(pane.changed_at_unix_ms)
                < 15_000;
            Ok(CodexSession {
                session_id,
                title: None,
                latest_user_message: None,
                latest_user_message_at_unix_ms: None,
                latest_agent_message: None,
                latest_agent_message_at_unix_ms: None,
                created_at_unix_ms: None,
                rollout_updated_at_unix_ms: None,
                cwd: None,
                model: None,
                cli_version: None,
                turn_count: 0,
                command_count: 0,
                file_change_count: 0,
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                observations: vec![CodexSessionObservation {
                    world_id: pane.world_id,
                    world_name: pane.world_name,
                    cwd: "/".into(),
                    repository_root: None,
                    repository_url: None,
                    git_branch: None,
                    git_context_checked_at_unix_ms: None,
                    git_context_error: None,
                    state: if active {
                        CodexSessionState::Working
                    } else {
                        CodexSessionState::NeedsAttention
                    },
                    is_compacting: false,
                    session_start_source: None,
                    target: ByobuTarget {
                        tmux_session: pane.tmux_session,
                        pane_id: pane.pane_id,
                    },
                    received_at_unix_ms: pane.observed_at_unix_ms,
                }],
            })
        })
        .collect()
}

fn validate_context(
    context: &str,
    sessions: Vec<CodexSession>,
    worlds: &[ShellWorld],
) -> Result<Vec<CodexCard>, String> {
    let mut cards = Vec::new();
    let mut session_ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for session in sessions {
        if !session_ids.insert(session.session_id) {
            return Err(invalid(
                context,
                "unique session_id",
                &session.session_id.to_string(),
            ));
        }
        if session
            .rollout_updated_at_unix_ms
            .is_some_and(|timestamp| timestamp < 0)
        {
            return Err(invalid(
                context,
                "nonnegative rollout timestamp",
                &session.session_id.to_string(),
            ));
        }
        if session.title.as_deref().is_some_and(|title| {
            title.is_empty() || title.len() > 640 || title.chars().any(char::is_control)
        }) {
            return Err(invalid(
                context,
                "short control-free session title",
                &session.session_id.to_string(),
            ));
        }
        if session
            .latest_user_message
            .as_deref()
            .is_some_and(|message| {
                message.is_empty() || message.len() > 640 || message.chars().any(char::is_control)
            })
        {
            return Err(invalid(
                context,
                "bounded control-free latest user message",
                &session.session_id.to_string(),
            ));
        }
        if session
            .latest_user_message_at_unix_ms
            .is_some_and(|timestamp| timestamp < 0)
            || session.latest_user_message_at_unix_ms.is_some()
                != session.latest_user_message.is_some()
        {
            return Err(invalid(
                context,
                "latest user message and nonnegative timestamp appear together",
                &session.session_id.to_string(),
            ));
        }
        if session
            .latest_agent_message
            .as_deref()
            .is_some_and(|message| {
                message.is_empty() || message.len() > 640 || message.chars().any(char::is_control)
            })
            || session
                .latest_agent_message_at_unix_ms
                .is_some_and(|timestamp| timestamp < 0)
            || session.latest_agent_message_at_unix_ms.is_some()
                != session.latest_agent_message.is_some()
        {
            return Err(invalid(
                context,
                "latest agent message and nonnegative timestamp appear together",
                &session.session_id.to_string(),
            ));
        }
        if session
            .created_at_unix_ms
            .is_some_and(|timestamp| timestamp < 0)
            || session.cwd.as_deref().is_some_and(|cwd| !valid_cwd(cwd))
            || session.model.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
            })
            || session.cli_version.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 64 || value.chars().any(char::is_control)
            })
        {
            return Err(invalid(
                context,
                "valid cached Codex session metadata",
                &session.session_id.to_string(),
            ));
        }
        if session.observations.is_empty() {
            let Some(timestamp) = session.rollout_updated_at_unix_ms else {
                return Err(invalid(
                    context,
                    "session has a rollout or observation",
                    &session.session_id.to_string(),
                ));
            };
            cards.push(CodexCard::rollout_only(
                context,
                session.session_id,
                timestamp,
                session.latest_user_message,
            ));
            continue;
        }
        for observation in session.observations {
            if observation.received_at_unix_ms < 0 {
                return Err(invalid(
                    context,
                    "nonnegative observation timestamp",
                    &session.session_id.to_string(),
                ));
            }
            if observation
                .git_context_checked_at_unix_ms
                .is_some_and(|timestamp| timestamp < 0)
                || observation
                    .git_context_error
                    .as_deref()
                    .is_some_and(|error| {
                        error.is_empty()
                            || error.len() > 1024
                            || error.chars().any(char::is_control)
                    })
            {
                return Err(invalid(
                    context,
                    "valid Git context health",
                    &observation.cwd,
                ));
            }
            if !valid_cwd(&observation.cwd) {
                return Err(invalid(
                    context,
                    "absolute control-free cwd",
                    &observation.cwd,
                ));
            }
            if observation
                .repository_root
                .as_deref()
                .is_some_and(|value| !valid_cwd(value))
                || observation.repository_url.as_deref().is_some_and(|value| {
                    value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control)
                })
                || observation.git_branch.as_deref().is_some_and(|value| {
                    value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control)
                })
            {
                return Err(invalid(context, "valid Git context", &observation.cwd));
            }
            if !valid_pane_id(&observation.target.pane_id) {
                return Err(invalid(
                    context,
                    "pane_id is % plus 1-16 ASCII digits",
                    &observation.target.pane_id,
                ));
            }
            let matching_worlds = worlds
                .iter()
                .filter(|world| {
                    world.identity.context == context
                        && world.identity.world_id == observation.world_id
                })
                .collect::<Vec<_>>();
            let [world] = matching_worlds.as_slice() else {
                return Err(invalid(
                    context,
                    "exactly one playback world matches (context, world_id)",
                    &observation.world_id.to_string(),
                ));
            };
            if world.world_name.as_str() != observation.world_name.as_str() {
                return Err(invalid(
                    context,
                    "world_name matches inventory world_id",
                    observation.world_name.as_str(),
                ));
            }
            let expected_tmux = "wt-host";
            if observation.target.tmux_session != expected_tmux {
                return Err(invalid(
                    context,
                    "tmux_session is wt-host",
                    &observation.target.tmux_session,
                ));
            }
            let identity = CodexCardIdentity::Observation {
                context: context.into(),
                session_id: session.session_id,
                world_id: observation.world_id,
                tmux_session: observation.target.tmux_session.clone(),
                pane_id: observation.target.pane_id.clone(),
            };
            if !identities.insert(identity.clone()) {
                return Err(invalid(
                    context,
                    "unique observation identity",
                    &format!("{}:{}", session.session_id, observation.target.pane_id),
                ));
            }
            cards.push(CodexCard {
                identity,
                context: context.into(),
                session_id: Some(session.session_id),
                timestamp: Some(observation.received_at_unix_ms),
                latest_user_message: session.latest_user_message.clone(),
                kind: CodexCardKind::Observation {
                    world_id: observation.world_id,
                    world_name: world.world_name.to_string(),
                    cwd: observation.cwd,
                    repository_root: observation.repository_root,
                    repository_url: observation.repository_url,
                    git_branch: observation.git_branch,
                    git_context_health: (observation.git_context_checked_at_unix_ms.is_some()
                        || observation.git_context_error.is_some())
                    .then(|| {
                        Box::new(GitContextHealth {
                            checked_at_unix_ms: observation.git_context_checked_at_unix_ms,
                            error: observation.git_context_error,
                        })
                    }),
                    state: observation.state,
                    is_compacting: observation.is_compacting,
                    target: observation.target,
                },
            });
        }
    }
    Ok(cards)
}

fn valid_cwd(value: &str) -> bool {
    value.starts_with('/') && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn valid_pane_id(value: &str) -> bool {
    value.strip_prefix('%').is_some_and(|number| {
        !number.is_empty() && number.len() <= 16 && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn invalid(context: &str, invariant: &str, value: &str) -> String {
    format!(
        "context {context}: failed invariant {invariant}; value {}",
        bounded_escaped(value.as_bytes())
    )
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

#[derive(Debug)]
pub(super) struct FocusResult {
    pub(super) action_id: Option<super::action_queue::ActionId>,
    pub(super) target: CodexOpenTarget,
    pub(super) control_path: PathBuf,
    pub(super) result: Result<(), String>,
    pub(super) open_world: bool,
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
    pub(super) fn start_action(
        &self,
        action_id: super::action_queue::ActionId,
        sessions: &SessionSet,
        model: &mut ShellModel,
        target: CodexOpenTarget,
    ) -> bool {
        let Some((index, alias)) = model.focus_route(&target) else {
            return false;
        };
        if !sessions.is_open(index) {
            return false;
        }
        self.start_request(
            Some(action_id),
            target,
            alias.to_owned(),
            sessions.control_path(index).to_owned(),
            true,
        );
        true
    }

    fn start_request(
        &self,
        action_id: Option<super::action_queue::ActionId>,
        target: CodexOpenTarget,
        alias: String,
        control_path: PathBuf,
        open_world: bool,
    ) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = focus(&target, &alias, &control_path).map_err(|error| error.to_string());
            let _ = sender.send(FocusResult {
                action_id,
                target,
                control_path,
                result,
                open_world,
            });
        });
    }

    pub(super) fn try_recv(&self) -> Option<FocusResult> {
        self.receiver.try_recv().ok()
    }
}

fn focus(target: &CodexOpenTarget, alias: &str, control_path: &Path) -> anyhow::Result<()> {
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
            &target.session_id.to_string(),
            &target.tmux_session,
            &target.pane_id,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let child = command
        .spawn()
        .map_err(|error| anyhow::anyhow!("start Codex focus helper: {error}"))?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let output = wt_client::transport::wait_with_output_timeout(child, remaining)
        .map_err(|error| anyhow::anyhow!("wait for Codex focus helper: {error}"))?;
    let expected = format!("{}:{}:0\n", target.tmux_session, target.pane_id);
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

pub(super) fn wait_for_control_master(
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

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
