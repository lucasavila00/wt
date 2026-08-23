use super::{map_store_error, AgentToolGateway, Service};
use std::collections::BTreeMap;
use wt_control_protocol::{
    ApiError, ByobuTarget, CodexSession, CodexSessionObservation, CodexSessionState, ErrorCode,
    InstanceName, Response,
};
use wt_guest::WorldWorker;
use wt_workload_registry::CodexSessionCatalogEntry;

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_codex_sessions(&self, owner: &str) -> Result<Response, ApiError> {
        let rollouts = self
            .store
            .list_codex_session_catalog()
            .map_err(map_store_error)?;
        let reports = self
            .store
            .list_codex_session_reports(owner)
            .map_err(map_store_error)?;
        Ok(Response::CodexSessions {
            sessions: merge_sessions(rollouts, reports)?,
        })
    }
}

fn merge_sessions(
    rollouts: Vec<CodexSessionCatalogEntry>,
    reports: Vec<wt_workload_registry::CodexSessionReport>,
) -> Result<Vec<CodexSession>, ApiError> {
    let mut sessions = rollouts
        .into_iter()
        .map(|rollout| {
            (
                rollout.session_id,
                CodexSession {
                    session_id: rollout.session_id,
                    title: rollout.title,
                    latest_user_message: rollout.latest_user_message,
                    latest_user_message_at_unix_ms: rollout.latest_user_message_at_unix_ms,
                    latest_agent_message: rollout.latest_agent_message,
                    latest_agent_message_at_unix_ms: rollout.latest_agent_message_at_unix_ms,
                    created_at_unix_ms: rollout.created_at_unix_ms,
                    rollout_updated_at_unix_ms: Some(rollout.rollout_updated_at_unix_ms),
                    cwd: rollout.cwd,
                    model: rollout.model,
                    cli_version: rollout.cli_version,
                    turn_count: rollout.turn_count,
                    command_count: rollout.command_count,
                    file_change_count: rollout.file_change_count,
                    input_tokens: rollout.input_tokens,
                    cached_input_tokens: rollout.cached_input_tokens,
                    output_tokens: rollout.output_tokens,
                    reasoning_output_tokens: rollout.reasoning_output_tokens,
                    observations: Vec::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for report in reports {
        let checkout = report.checkout;
        let session = sessions
            .entry(report.session_id)
            .or_insert_with(|| empty_session(report.session_id));
        session.observations.push(CodexSessionObservation {
            world_id: report.world_id,
            world_name: InstanceName::parse(report.world_name).map_err(|error| {
                ApiError::new(
                    ErrorCode::Internal,
                    format!("invalid session world: {error}"),
                )
            })?,
            cwd: report.cwd,
            repository_root: checkout
                .as_ref()
                .and_then(|state| state.repository_root.clone()),
            repository_url: checkout
                .as_ref()
                .and_then(|state| state.repository_url.clone()),
            git_branch: checkout.as_ref().and_then(|state| state.branch.clone()),
            git_context_checked_at_unix_ms: checkout.as_ref().map(|state| state.checked_at_unix_ms),
            git_context_error: checkout.and_then(|state| state.error),
            state: match report.state {
                wt_workload_registry::CodexSessionState::Unknown => CodexSessionState::Unknown,
                wt_workload_registry::CodexSessionState::Working => CodexSessionState::Working,
                wt_workload_registry::CodexSessionState::NeedsAttention => {
                    CodexSessionState::NeedsAttention
                }
                wt_workload_registry::CodexSessionState::Inactive => CodexSessionState::Inactive,
            },
            is_compacting: report.is_compacting,
            session_start_source: report.session_start_source,
            target: ByobuTarget {
                tmux_session: report.tmux_session,
                pane_id: report.pane_id,
            },
            received_at_unix_ms: report.received_at_unix_ms,
        });
    }
    for session in sessions.values_mut() {
        session.observations.sort_by(|left, right| {
            right
                .received_at_unix_ms
                .cmp(&left.received_at_unix_ms)
                .then_with(|| left.world_name.cmp(&right.world_name))
        });
    }
    let mut sessions = sessions.into_values().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        session_updated_at(right)
            .cmp(&session_updated_at(left))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    Ok(sessions)
}

fn empty_session(session_id: uuid::Uuid) -> CodexSession {
    CodexSession {
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
        observations: Vec::new(),
    }
}

fn session_updated_at(session: &CodexSession) -> i64 {
    session
        .observations
        .first()
        .map(|observation| observation.received_at_unix_ms)
        .into_iter()
        .chain(session.rollout_updated_at_unix_ms)
        .max()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn reports_keep_the_observed_state_and_complete_target() {
        let session_id = Uuid::new_v4();
        let sessions = merge_sessions(
            Vec::new(),
            vec![wt_workload_registry::CodexSessionReport {
                world_id: Uuid::new_v4(),
                world_name: "example".into(),
                session_id,
                cwd: "/home/wt/project".into(),
                checkout: Some(wt_workload_registry::CodexCheckoutState {
                    repository_root: Some("/home/wt/project".into()),
                    repository_url: Some("git@github.com:acme/project.git".into()),
                    provider_host: Some("github.com".into()),
                    repository: Some("acme/project".into()),
                    branch: Some("wt/cards".into()),
                    checked_at_unix_ms: 1,
                    error: None,
                }),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
                state: wt_workload_registry::CodexSessionState::Working,
                is_compacting: false,
                session_start_source: None,
                received_at_unix_ms: 2,
            }],
        )
        .unwrap();

        assert_eq!(sessions[0].observations.len(), 1);
        assert_eq!(
            sessions[0].observations[0].state,
            CodexSessionState::Working
        );
        assert_eq!(sessions[0].observations[0].target.pane_id, "%1");
    }

    #[test]
    fn preserves_every_world_observation_for_one_session() {
        let session_id = Uuid::new_v4();
        let reports = [("first", "%1", 10), ("second", "%2", 20)]
            .into_iter()
            .map(|(world_name, pane_id, received_at_unix_ms)| {
                wt_workload_registry::CodexSessionReport {
                    world_id: Uuid::new_v4(),
                    world_name: world_name.into(),
                    session_id,
                    cwd: "/home/wt/project".into(),
                    checkout: None,
                    tmux_session: "wt-host".into(),
                    pane_id: pane_id.into(),
                    state: wt_workload_registry::CodexSessionState::Working,
                    is_compacting: false,
                    session_start_source: None,
                    received_at_unix_ms,
                }
            })
            .collect();

        let sessions = merge_sessions(Vec::new(), reports).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].observations.len(), 2);
        assert_eq!(sessions[0].observations[0].world_name.as_str(), "second");
        assert_eq!(sessions[0].observations[1].world_name.as_str(), "first");
    }
}
