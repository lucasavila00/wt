use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::shell) enum CodexCardIdentity {
    Observation {
        context: String,
        session_id: Uuid,
        world_id: Uuid,
        tmux_session: String,
        pane_id: String,
    },
    RolloutOnly {
        context: String,
        session_id: Uuid,
    },
    ContextError {
        context: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::shell) struct CodexOpenTarget {
    pub(in crate::shell) identity: CodexCardIdentity,
    pub(in crate::shell) context: String,
    pub(in crate::shell) session_id: Uuid,
    pub(in crate::shell) world_id: Uuid,
    pub(in crate::shell) tmux_session: String,
    pub(in crate::shell) pane_id: String,
}

#[derive(Clone, Debug)]
pub(in crate::shell) struct GitContextHealth {
    pub(in crate::shell) checked_at_unix_ms: Option<i64>,
    pub(in crate::shell) error: Option<String>,
}

impl GitContextHealth {
    pub(in crate::shell) fn warning(&self) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(format!("Git state unavailable: {error}"));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())?;
        self.checked_at_unix_ms
            .is_some_and(|checked| now.saturating_sub(checked) > 30_000)
            .then(|| "Git state stale".into())
    }
}

#[derive(Clone, Debug)]
pub(in crate::shell) enum CodexCardKind {
    Observation {
        world_id: Uuid,
        world_name: String,
        cwd: String,
        repository_root: Option<String>,
        repository_url: Option<String>,
        git_branch: Option<String>,
        git_context_health: Option<Box<GitContextHealth>>,
        state: CodexSessionState,
        is_compacting: bool,
        session_start_source: Option<String>,
        target: ByobuTarget,
    },
    RolloutOnly,
    ContextError {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(in crate::shell) struct CodexCard {
    pub(in crate::shell) identity: CodexCardIdentity,
    pub(in crate::shell) context: String,
    pub(in crate::shell) session_id: Option<Uuid>,
    pub(in crate::shell) timestamp: Option<i64>,
    pub(in crate::shell) latest_user_message: Option<String>,
    pub(in crate::shell) kind: CodexCardKind,
}

impl CodexCard {
    pub(in crate::shell) fn rollout_only(
        context: &str,
        session_id: Uuid,
        timestamp: i64,
        latest_user_message: Option<String>,
    ) -> Self {
        Self {
            identity: CodexCardIdentity::RolloutOnly {
                context: context.into(),
                session_id,
            },
            context: context.into(),
            session_id: Some(session_id),
            timestamp: Some(timestamp),
            latest_user_message,
            kind: CodexCardKind::RolloutOnly,
        }
    }

    pub(in crate::shell) fn context_error(context: &str, message: String) -> Self {
        Self {
            identity: CodexCardIdentity::ContextError {
                context: context.into(),
            },
            context: context.into(),
            session_id: None,
            timestamp: None,
            latest_user_message: None,
            kind: CodexCardKind::ContextError { message },
        }
    }

    pub(in crate::shell) fn open_target(&self) -> Option<CodexOpenTarget> {
        let CodexCardKind::Observation {
            world_id,
            state,
            target,
            ..
        } = &self.kind
        else {
            return None;
        };
        if *state == CodexSessionState::Inactive {
            return None;
        }
        Some(CodexOpenTarget {
            identity: self.identity.clone(),
            context: self.context.clone(),
            session_id: self.session_id.expect("observation card has session ID"),
            world_id: *world_id,
            tmux_session: target.tmux_session.clone(),
            pane_id: target.pane_id.clone(),
        })
    }

    pub(in crate::shell) fn sort_rank(&self) -> u8 {
        match &self.kind {
            CodexCardKind::Observation { state, .. } => match state {
                CodexSessionState::NeedsAttention => 0,
                CodexSessionState::Working => 1,
                CodexSessionState::Unknown => 2,
                CodexSessionState::Inactive => 3,
            },
            CodexCardKind::RolloutOnly => 4,
            CodexCardKind::ContextError { .. } => 5,
        }
    }

    pub(in crate::shell) fn timestamp(&self) -> i64 {
        self.timestamp.unwrap_or_default()
    }

    pub(in crate::shell) fn disabled_reason(&self) -> Option<&'static str> {
        match &self.kind {
            CodexCardKind::Observation {
                state: CodexSessionState::Inactive,
                ..
            } => Some("session ended"),
            CodexCardKind::RolloutOnly => Some("session is not open in a WT pane"),
            CodexCardKind::ContextError { .. } => Some("context data rejected"),
            CodexCardKind::Observation { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::shell) enum ControlCommand {
    NewWorld,
    DeleteWorld,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::shell) enum ControlAction {
    Command(ControlCommand),
    OpenCodex(Box<CodexOpenTarget>),
}
