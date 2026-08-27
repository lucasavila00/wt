use wt_control_protocol::WorldId;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::shell) enum PaneCardIdentity {
    Observation {
        context: String,
        world_id: WorldId,
        tmux_session: String,
        pane_id: String,
    },
    ContextError {
        context: String,
    },
}

#[derive(Clone, Debug)]
pub(in crate::shell) enum PaneCardKind {
    Observation {
        world_name: String,
        tmux_session: String,
        pane_id: String,
        changed_at_unix_ms: i64,
    },
    ContextError {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(in crate::shell) struct PaneCard {
    pub(in crate::shell) identity: PaneCardIdentity,
    pub(in crate::shell) context: String,
    pub(in crate::shell) observed_at_unix_ms: Option<i64>,
    pub(in crate::shell) kind: PaneCardKind,
}

impl PaneCard {
    pub(in crate::shell) fn world_id(&self) -> Option<WorldId> {
        match self.identity {
            PaneCardIdentity::Observation { world_id, .. } => Some(world_id),
            PaneCardIdentity::ContextError { .. } => None,
        }
    }

    pub(in crate::shell) fn context_error(context: &str, message: String) -> Self {
        Self {
            identity: PaneCardIdentity::ContextError {
                context: context.into(),
            },
            context: context.into(),
            observed_at_unix_ms: None,
            kind: PaneCardKind::ContextError { message },
        }
    }

    pub(in crate::shell) fn sort_rank(&self) -> u8 {
        match self.kind {
            PaneCardKind::Observation { .. } => 0,
            PaneCardKind::ContextError { .. } => 1,
        }
    }

    pub(in crate::shell) fn timestamp(&self) -> i64 {
        self.observed_at_unix_ms.unwrap_or_default()
    }

    pub(in crate::shell) fn changed_recently(&self) -> bool {
        !self.is_stale()
            && matches!(
                self.kind,
                PaneCardKind::Observation {
                    changed_at_unix_ms,
                    ..
                } if Self::now_unix_ms().saturating_sub(changed_at_unix_ms) < 15_000
            )
    }

    pub(in crate::shell) fn is_stale(&self) -> bool {
        self.observed_at_unix_ms
            .is_some_and(|observed_at| Self::now_unix_ms().saturating_sub(observed_at) > 30_000)
    }

    pub(in crate::shell) fn disabled_reason(&self) -> Option<&'static str> {
        match self.kind {
            PaneCardKind::Observation { .. } => None,
            PaneCardKind::ContextError { .. } => Some("context data rejected"),
        }
    }

    fn now_unix_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .unwrap_or_default()
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
    OpenPane(PaneCardIdentity),
}
