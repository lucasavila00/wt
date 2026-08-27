use super::control::{PaneCard, PaneCardIdentity, PaneCardKind};
use super::model::ShellWorld;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

const MAX_PANE_ROWS: usize = 6;

pub(super) fn status(world: &ShellWorld, idle: bool) -> (&'static str, Color, String) {
    match (world.status, idle) {
        (_, true) => (
            "󰚩",
            Color::Yellow,
            "IDLE · NO RECENT PANE CHANGE".to_owned(),
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

pub(super) fn is_idle(world: &ShellWorld, cards: &[PaneCard]) -> bool {
    if cards.iter().any(|card| {
        matches!(
            &card.kind,
            PaneCardKind::ContextError if card.context == world.identity.context
        )
    }) {
        return false;
    }
    let panes = cards
        .iter()
        .filter(|card| belongs_to_world(card, world))
        .collect::<Vec<_>>();
    !panes.iter().any(|card| card.is_stale()) && !panes.iter().any(|card| card.changed_recently())
}

pub(super) fn pane_lines(world: &ShellWorld, cards: &[PaneCard]) -> Vec<Line<'static>> {
    let lines = cards
        .iter()
        .filter_map(|card| {
            let PaneCardIdentity::Observation {
                tmux_session,
                pane_id,
                ..
            } = &card.identity
            else {
                return None;
            };
            belongs_to_world(card, world).then(|| {
                Line::from(vec![
                    Span::styled("Codex ", super::render::muted_style()),
                    Span::raw(format!("{tmux_session}:{pane_id} · {}", pane_status(card))),
                ])
            })
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        vec![Line::styled(
            "Codex no observed Byobu panes",
            super::render::muted_style(),
        )]
    } else {
        let mut visible = lines;
        if visible.len() > MAX_PANE_ROWS {
            let hidden = visible.len() - (MAX_PANE_ROWS - 1);
            visible.truncate(MAX_PANE_ROWS - 1);
            visible.push(Line::styled(
                format!("Codex +{hidden} more panes"),
                super::render::muted_style(),
            ));
        }
        visible
    }
}

pub(super) fn git_lines(world: &ShellWorld) -> Vec<Line<'static>> {
    let repositories = match &world.git_activity {
        super::git_activity::RepositoryActivity::Loading => {
            return vec![Line::styled(
                "Git activity loading…",
                super::render::muted_style(),
            )]
        }
        super::git_activity::RepositoryActivity::Unavailable => {
            return vec![Line::styled(
                "Git activity unavailable",
                super::render::muted_style(),
            )]
        }
        super::git_activity::RepositoryActivity::Loaded(repositories)
            if repositories.is_empty() =>
        {
            return vec![Line::styled(
                "Git no recorded interactions",
                super::render::muted_style(),
            )]
        }
        super::git_activity::RepositoryActivity::Loaded(repositories) => repositories,
    };
    repositories
        .iter()
        .map(|repository| {
            let interaction = if repository.wrote { "write" } else { "read" };
            Line::from(vec![
                Span::styled(format!("Git {interaction} "), super::render::muted_style()),
                Span::raw(repository.target.clone()),
            ])
        })
        .collect()
}

fn belongs_to_world(card: &PaneCard, world: &ShellWorld) -> bool {
    matches!(
        &card.identity,
        PaneCardIdentity::Observation {
            context, world_id, ..
        } if context == &world.identity.context && world_id == &world.identity.world_id
    )
}

fn pane_status(card: &PaneCard) -> &'static str {
    if card.is_stale() {
        "STALE"
    } else if card.changed_recently() {
        "CHANGING"
    } else {
        "STATIC"
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
    details: &[Line<'static>],
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
    lines.extend_from_slice(details);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    Paragraph::new(lines).render(rows[0], buffer);
    Paragraph::new(footer)
        .style(super::render::muted_style())
        .render(rows[1], buffer);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_unix_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap()
    }

    fn observation(world: &ShellWorld, pane_id: &str, changed_at_unix_ms: i64) -> PaneCard {
        let now = now_unix_ms();
        PaneCard {
            identity: PaneCardIdentity::Observation {
                context: world.identity.context.clone(),
                world_id: world.identity.world_id,
                tmux_session: "wt-host".into(),
                pane_id: pane_id.into(),
            },
            context: world.identity.context.clone(),
            created_at_unix_ms: Some(now),
            observed_at_unix_ms: Some(now),
            kind: PaneCardKind::Observation {
                world_name: world.world_name.to_string(),
                changed_at_unix_ms,
                frame: None,
            },
        }
    }

    #[test]
    fn lists_the_observed_panes_for_its_world() {
        let world = ShellWorld::test("ars.dev", 1);
        let now = now_unix_ms();
        let lines = pane_lines(
            &world,
            &[
                observation(&world, "%1", now),
                observation(&world, "%2", now - 16_000),
            ],
        );

        insta::assert_snapshot!(
            lines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            @r###"
        Codex wt-host:%1 · CHANGING
        Codex wt-host:%2 · STATIC
        "###
        );
    }

    #[test]
    fn lists_git_interactions_before_empty_pane_state() {
        let mut world = ShellWorld::test("ars.dev", 1);
        world.git_activity = super::super::git_activity::RepositoryActivity::Loaded(vec![
            super::super::git_activity::RepositoryInteraction {
                target: "github.com/owner/write".into(),
                wrote: true,
            },
            super::super::git_activity::RepositoryInteraction {
                target: "github.com/owner/read".into(),
                wrote: false,
            },
        ]);

        insta::assert_snapshot!(
            git_lines(&world)
                .into_iter()
                .chain(pane_lines(&world, &[]))
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            @r###"
        Git write github.com/owner/write
        Git read github.com/owner/read
        Codex no observed Byobu panes
        "###
        );
    }

    #[test]
    fn marks_worlds_idle_without_a_recent_pane_change() {
        let world = ShellWorld::test("ars.dev", 1);
        let now = now_unix_ms();
        let static_pane = observation(&world, "%1", now - 16_000);
        let changing_pane = observation(&world, "%2", now);

        assert!(is_idle(&world, &[]));
        assert!(is_idle(&world, &[static_pane]));
        assert!(!is_idle(&world, &[changing_pane]));
        let mut stale_pane = observation(&world, "%3", 0);
        stale_pane.observed_at_unix_ms = Some(0);
        assert!(!is_idle(&world, &[stale_pane]));
        assert!(!is_idle(&world, &[PaneCard::context_error("ars")]));
    }

    #[test]
    fn bounds_long_pane_lists_with_an_overflow_row() {
        let world = ShellWorld::test("ars.dev", 1);
        let now = now_unix_ms();
        let panes = (1..=8)
            .map(|index| observation(&world, &format!("%{index}"), now))
            .collect::<Vec<_>>();

        insta::assert_snapshot!(
            pane_lines(&world, &panes)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            @r###"
        Codex wt-host:%1 · CHANGING
        Codex wt-host:%2 · CHANGING
        Codex wt-host:%3 · CHANGING
        Codex wt-host:%4 · CHANGING
        Codex wt-host:%5 · CHANGING
        Codex +3 more panes
        "###
        );
    }
}
