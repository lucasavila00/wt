use super::control::{CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget};
pub(super) use super::model::ShellWorld;
use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
#[cfg(test)]
use uuid::Uuid;
use wt_client::config::ClientConfig;
use wt_client::inventory::ContextInstance;
use wt_control_protocol::{CodexSession, Instance};

#[derive(Debug)]
pub(super) enum CodexContextSnapshot {
    Sessions {
        context: String,
        sessions: Vec<CodexSession>,
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
    pub(super) fn from_inventory(item: &ContextInstance) -> Self {
        let mut world = Self::from_instance(&item.context, &item.instance);
        world.resources =
            wt_client::inventory::format_resources(&item.instance, item.disk_usage_bytes);
        world.detail = wt_client::inventory::format_detail(item);
        world
    }

    pub(super) fn from_instance(context: &str, instance: &Instance) -> Self {
        let qualified_name = format!("{context}.{}", instance.name);
        let control_alias = format!("{qualified_name}-direct");
        Self {
            identity: super::model::WorldIdentity {
                context: context.into(),
                id: instance.id,
            },
            name: qualified_name,
            instance_name: instance.name.clone(),
            control_alias,
            status: instance.status,
            resources: wt_client::inventory::format_resources(instance, None),
            detail: instance.last_error.as_deref().unwrap_or("-").into(),
        }
    }

    #[cfg(test)]
    pub(super) fn test(name: &str, index: u128) -> Self {
        let (context, world_name) = name.split_once('.').unwrap_or(("local", name));
        Self {
            identity: super::model::WorldIdentity {
                context: context.into(),
                id: Uuid::from_u128(index),
            },
            name: name.into(),
            instance_name: wt_control_protocol::InstanceName::parse(world_name).unwrap(),
            control_alias: format!("{name}-direct"),
            status: wt_control_protocol::InstanceStatus::Running,
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
            match wt_client::transport::call_codex_sessions_with_timeout_until(
                context,
                super::CONTEXT_REQUEST_TIMEOUT,
                cancelled,
            ) {
                Ok(sessions) => CodexContextSnapshot::Sessions {
                    context: context.name.clone(),
                    sessions,
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
            CodexContextSnapshot::Sessions { context, sessions } => {
                match validate_context(&context, sessions, worlds) {
                    Ok(mut context_cards) => cards.append(&mut context_cards),
                    Err(message) => cards.push(CodexCard::context_error(&context, message)),
                }
            }
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
                    world.identity.context == context && world.identity.id == observation.world_id
                })
                .collect::<Vec<_>>();
            let [world] = matching_worlds.as_slice() else {
                return Err(invalid(
                    context,
                    "exactly one playback world matches (context, world_id)",
                    &observation.world_id.to_string(),
                ));
            };
            if world.instance_name.as_str() != observation.world_name.as_str() {
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
                    world_name: world.instance_name.to_string(),
                    cwd: observation.cwd,
                    repository_root: observation.repository_root,
                    repository_url: observation.repository_url,
                    git_branch: observation.git_branch,
                    state: observation.state,
                    session_start_source: observation.session_start_source,
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
    pub(super) target: CodexOpenTarget,
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
    pub(super) fn start(&self, target: CodexOpenTarget, alias: String) {
        self.start_request(target, alias, true);
    }

    pub(super) fn start_live(&self, target: CodexOpenTarget, alias: String) {
        self.start_request(target, alias, false);
    }

    fn start_request(&self, target: CodexOpenTarget, alias: String, open_world: bool) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = focus(&target, &alias).map_err(|error| error.to_string());
            let _ = sender.send(FocusResult {
                target,
                result,
                open_world,
            });
        });
    }

    pub(super) fn try_recv(&self) -> Option<FocusResult> {
        self.receiver.try_recv().ok()
    }
}

fn focus(target: &CodexOpenTarget, alias: &str) -> anyhow::Result<()> {
    let mut command = Command::new("ssh");
    command
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "BatchMode=yes",
            "--",
            alias,
            "/usr/local/bin/wt-codex-integration",
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
    let output = wt_client::transport::wait_with_output_timeout(child, Duration::from_secs(15))
        .map_err(|error| anyhow::anyhow!("wait for Codex focus helper: {error}"))?;
    let expected = format!(
        "{}:{}:{}:0\n",
        target.tmux_session, target.pane_id, target.session_id
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use wt_control_protocol::{
        ByobuTarget, CodexSessionObservation, CodexSessionState, InstanceName,
    };

    fn session(world: &ShellWorld, cwd: &str) -> CodexSession {
        CodexSession {
            session_id: Uuid::from_u128(10),
            title: Some("Improve session cards".into()),
            latest_user_message: Some("Make the cards taller and show the latest request".into()),
            latest_user_message_at_unix_ms: Some(9),
            latest_agent_message: None,
            latest_agent_message_at_unix_ms: None,
            created_at_unix_ms: None,
            rollout_updated_at_unix_ms: Some(10),
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
                world_id: world.identity.id,
                world_name: world.instance_name.clone(),
                cwd: cwd.into(),
                repository_root: Some("/home/wt/project".into()),
                repository_url: Some("git@github.com:acme/project.git".into()),
                git_branch: Some("wt/cards".into()),
                state: CodexSessionState::NeedsAttention,
                session_start_source: None,
                target: ByobuTarget {
                    tmux_session: "wt-host".into(),
                    pane_id: "%1".into(),
                },
                received_at_unix_ms: 20,
            }],
        }
    }

    #[test]
    fn validates_complete_context_before_creating_cards() {
        let world = ShellWorld::test("ars.dev", 1);
        let cards = validate_context(
            "ars",
            vec![session(&world, "/home/wt/project")],
            std::slice::from_ref(&world),
        )
        .unwrap();
        assert_eq!(cards.len(), 1);
        assert!(cards[0].open_target().is_some());

        insta::assert_snapshot!(
            validate_context("ars", vec![session(&world, "relative")], &[world])
                .unwrap_err(),
            @"context ars: failed invariant absolute control-free cwd; value relative"
        );
    }

    #[test]
    fn rejects_world_name_and_tmux_mismatches() {
        let world = ShellWorld::test("ars.dev", 1);
        let mut wrong_name = session(&world, "/home/wt/project");
        wrong_name.observations[0].world_name = InstanceName::parse("other").unwrap();
        insta::assert_snapshot!(
            validate_context("ars", vec![wrong_name], std::slice::from_ref(&world)).unwrap_err(),
            @"context ars: failed invariant world_name matches inventory world_id; value other"
        );

        let mut wrong_tmux = session(&world, "/home/wt/project");
        wrong_tmux.observations[0].target.tmux_session = "other".into();
        insta::assert_snapshot!(
            validate_context("ars", vec![wrong_tmux], &[world]).unwrap_err(),
            @"context ars: failed invariant tmux_session is wt-host; value other"
        );
    }

    #[test]
    fn rejects_duplicate_sessions_and_negative_timestamps() {
        let world = ShellWorld::test("ars.dev", 1);
        let valid = session(&world, "/home/wt/project");
        insta::assert_snapshot!(
            validate_context(
                "ars",
                vec![valid.clone(), valid.clone()],
                std::slice::from_ref(&world)
            )
            .unwrap_err(),
            @"context ars: failed invariant unique session_id; value 00000000-0000-0000-0000-00000000000a"
        );

        let mut negative = valid;
        negative.observations[0].received_at_unix_ms = -1;
        insta::assert_snapshot!(
            validate_context("ars", vec![negative], &[world]).unwrap_err(),
            @"context ars: failed invariant nonnegative observation timestamp; value 00000000-0000-0000-0000-00000000000a"
        );
    }

    #[test]
    fn query_failures_preserve_the_error_instead_of_creating_cards() {
        let result = cards(
            vec![CodexContextSnapshot::Failure {
                message: "context ars could not be queried: server rejected the request".into(),
            }],
            &[],
        );

        assert!(result.cards.is_empty());
        assert_eq!(
            result.failures,
            ["context ars could not be queried: server rejected the request"]
        );
    }
}
