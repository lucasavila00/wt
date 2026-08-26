use super::control::{CodexCard, CodexCardKind};
use super::model::ShellWorld;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

pub(super) fn status(world: &ShellWorld, idle: bool) -> (&'static str, Color, String) {
    match (world.status, idle) {
        (_, true) => (
            "󰚩",
            Color::Yellow,
            "STATIC · NO RECENT PANE CHANGE".to_owned(),
        ),
        (wt_control_protocol::WorldStatus::Running, false) => {
            ("󰐊", Color::Green, "RUNNING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Provisioning, false) => {
            ("󰔟", Color::Yellow, "PROVISIONING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Stopped, false) => {
            ("󰅖", Color::Reset, "STOPPED".to_owned())
        }
        (wt_control_protocol::WorldStatus::Destroying, false) => {
            ("󰩹", Color::Yellow, "DESTROYING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Error, false) => ("󰅚", Color::Red, "ERROR".to_owned()),
    }
}

pub(super) fn has_active_codex_session(world: &ShellWorld, cards: &[CodexCard]) -> bool {
    cards.iter().any(|card| {
        matches!(
            &card.kind,
            CodexCardKind::Observation {
                world_id,
                state,
                ..
            } if *world_id == world.identity.world_id
                && card.context == world.identity.context
                && *state != wt_control_protocol::CodexSessionState::Inactive
        )
    })
}

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
            (*world_id == world.identity.world_id && card.context == world.identity.context)
                .then_some((
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
            Span::styled("  Pane ", super::render::muted_style()),
            Span::raw(format!(
                "{session} · {} · {}",
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
        wt_control_protocol::CodexSessionState::Working => "CHANGING",
        wt_control_protocol::CodexSessionState::NeedsAttention => "STATIC",
        wt_control_protocol::CodexSessionState::Inactive => "INACTIVE",
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    buffer: &mut Buffer,
    area: Rect,
    icon: &str,
    color: Color,
    status: &str,
    name: &str,
    resources: &str,
    detail: Option<&str>,
    codex: &[Line<'static>],
    selected: bool,
    footer: &str,
    show_actions: bool,
) {
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_style(super::render::selected_card_border_style(selected))
        .title(Span::styled(
            format!(" {icon} {status} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
    if show_actions {
        block = block.title(
            Line::styled(
                super::world_menu::CARD_LABEL,
                Style::new().add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Right),
        );
    }
    let inner = block.inner(area);
    block.render(area, buffer);
    let mut lines = vec![
        Line::from(name.to_owned()),
        Line::from(resources.to_owned()),
    ];
    if let Some(detail) = detail {
        lines.push(Line::from(detail.to_owned()));
    }
    lines.extend_from_slice(codex);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(rows[0], buffer);
    Paragraph::new(footer)
        .style(super::render::muted_style())
        .render(rows[1], buffer);
}
