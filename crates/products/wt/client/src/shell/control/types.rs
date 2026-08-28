use wt_control_protocol::{PaneFrame, PaneRender, WorldId};

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
        changed_at_unix_ms: i64,
        cwd: String,
        git_branch: Option<String>,
        render: PaneRender,
    },
    ContextError,
}

#[derive(Clone, Debug)]
pub(in crate::shell) struct PaneCard {
    pub(in crate::shell) identity: PaneCardIdentity,
    pub(in crate::shell) context: String,
    pub(in crate::shell) created_at_unix_ms: Option<i64>,
    pub(in crate::shell) observed_at_unix_ms: Option<i64>,
    pub(in crate::shell) classified_at_unix_ms: i64,
    pub(in crate::shell) kind: PaneCardKind,
}

impl PaneCard {
    #[cfg(test)]
    pub(in crate::shell) fn world_id(&self) -> Option<WorldId> {
        match self.identity {
            PaneCardIdentity::Observation { world_id, .. } => Some(world_id),
            PaneCardIdentity::ContextError { .. } => None,
        }
    }

    pub(in crate::shell) fn context_error(context: &str) -> Self {
        Self {
            identity: PaneCardIdentity::ContextError {
                context: context.into(),
            },
            context: context.into(),
            created_at_unix_ms: None,
            observed_at_unix_ms: None,
            classified_at_unix_ms: 0,
            kind: PaneCardKind::ContextError,
        }
    }

    pub(in crate::shell) fn sort_rank(&self) -> u8 {
        match self.kind {
            PaneCardKind::Observation { .. } if self.changed_recently() => 1,
            PaneCardKind::Observation { .. } => 0,
            PaneCardKind::ContextError => 2,
        }
    }

    pub(in crate::shell) fn created_at_unix_ms(&self) -> i64 {
        self.created_at_unix_ms.unwrap_or_default()
    }

    pub(in crate::shell) fn window_index(&self) -> i64 {
        match &self.kind {
            PaneCardKind::Observation { render, .. } => render.window_index,
            PaneCardKind::ContextError => i64::MAX,
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
                } if self.classified_at_unix_ms.saturating_sub(changed_at_unix_ms) < 15_000
            )
    }

    pub(in crate::shell) fn is_stale(&self) -> bool {
        self.observed_at_unix_ms.is_some_and(|observed_at| {
            self.classified_at_unix_ms.saturating_sub(observed_at) >= 60_000
        })
    }

    pub(in crate::shell) fn disabled_reason(&self) -> Option<&'static str> {
        match self.kind {
            PaneCardKind::Observation { .. } => None,
            PaneCardKind::ContextError => Some("context data rejected"),
        }
    }

    pub(in crate::shell) fn frame(&self) -> Option<&PaneFrame> {
        match &self.kind {
            PaneCardKind::Observation { render, .. } => Some(&render.frame),
            PaneCardKind::ContextError => None,
        }
    }

    pub(in crate::shell) fn location(&self) -> Option<String> {
        match &self.kind {
            PaneCardKind::Observation {
                cwd, git_branch, ..
            } => Some(match git_branch {
                Some(git_branch) => format!("{cwd} · {git_branch}"),
                None => cwd.clone(),
            }),
            PaneCardKind::ContextError => None,
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
    OpenPane(PaneCardIdentity),
}
