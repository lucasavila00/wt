use super::control::{CodexCard, CodexCardKind};
use super::model::ShellWorld;
use ratatui::text::{Line, Span};

pub(super) fn codex_lines(world: &ShellWorld, cards: &[CodexCard]) -> Vec<Line<'static>> {
    let mut observations = cards
        .iter()
        .filter_map(|card| {
            let CodexCardKind::Observation {
                world_id,
                cwd,
                repository_root,
                repository_url,
                git_branch,
                state,
                ..
            } = &card.kind
            else {
                return None;
            };
            (*world_id == world.identity.id && card.context == world.identity.context).then_some((
                card,
                cwd,
                repository_root,
                repository_url,
                git_branch,
                state,
            ))
        })
        .collect::<Vec<_>>();
    observations.sort_by(|left, right| {
        let left_root = left.2.as_deref().unwrap_or(left.1);
        let right_root = right.2.as_deref().unwrap_or(right.1);
        left_root.cmp(right_root).then_with(|| {
            std::cmp::Reverse(left.0.timestamp()).cmp(&std::cmp::Reverse(right.0.timestamp()))
        })
    });

    let mut lines = Vec::new();
    let mut checkout = None;
    for (card, cwd, repository_root, repository_url, git_branch, state) in observations {
        let root = repository_root.as_deref().unwrap_or(cwd);
        if checkout != Some(root) {
            checkout = Some(root);
            let repository = repository_url
                .as_deref()
                .and_then(super::render::repository_name)
                .unwrap_or(root);
            let label = git_branch.as_deref().map_or_else(
                || repository.to_owned(),
                |branch| format!("{repository} · {branch}"),
            );
            lines.push(Line::from(vec![
                Span::styled("Checkout ", super::render::muted_style()),
                Span::raw(label),
            ]));
        }
        let session = card
            .session_id
            .map(|id| id.to_string()[..8].to_owned())
            .unwrap_or_else(|| "unknown".into());
        lines.push(Line::from(vec![
            Span::styled("  Codex ", super::render::muted_style()),
            Span::raw(format!(
                "session {session} · {} · {}",
                state_label(*state),
                card.timestamp
                    .map(super::render::relative_age)
                    .unwrap_or_else(|| "unknown".into())
            )),
        ]));
    }
    lines
}

fn state_label(state: wt_control_protocol::CodexSessionState) -> &'static str {
    match state {
        wt_control_protocol::CodexSessionState::Unknown => "UNKNOWN",
        wt_control_protocol::CodexSessionState::Working => "WORKING",
        wt_control_protocol::CodexSessionState::NeedsAttention => "NEEDS ATTENTION",
        wt_control_protocol::CodexSessionState::Inactive => "INACTIVE",
    }
}
