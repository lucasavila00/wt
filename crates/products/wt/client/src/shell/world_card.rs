use super::control::{PaneCard, PaneCardIdentity, PaneCardKind};
use super::model::ShellWorld;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

const MAX_PANE_ROWS: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Liveness {
    Current,
    Idle,
    ConnectionLost,
}

pub(super) fn status(world: &ShellWorld, liveness: Liveness) -> (&'static str, Color, String) {
    match (world.status, liveness) {
        (_, Liveness::Idle) => (
            "󰚩",
            Color::Yellow,
            "IDLE · NO RECENT PANE CHANGE".to_owned(),
        ),
        (wt_control_protocol::WorldStatus::Running, Liveness::ConnectionLost) => (
            "󰅚",
            Color::Yellow,
            "CONNECTION LOST · NO PANE UPDATE".to_owned(),
        ),
        (wt_control_protocol::WorldStatus::Running, Liveness::Current) => {
            ("󰐊", Color::Green, "RUNNING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Provisioning, _) => {
            ("󰔟", Color::Yellow, "PROVISIONING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Stopped, _) => ("󰅖", Color::Reset, "STOPPED".to_owned()),
        (wt_control_protocol::WorldStatus::Destroying, _) => {
            ("󰩹", Color::Yellow, "DESTROYING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Error, _) => ("󰅚", Color::Red, "ERROR".to_owned()),
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

pub(super) fn has_lost_connection(world: &ShellWorld, cards: &[PaneCard]) -> bool {
    cards
        .iter()
        .any(|card| belongs_to_world(card, world) && card.is_stale())
}

pub(super) fn pane_lines(world: &ShellWorld, cards: &[PaneCard]) -> Vec<Line<'static>> {
    let lines = cards
        .iter()
        .filter_map(|card| {
            let PaneCardIdentity::Observation { .. } = &card.identity else {
                return None;
            };
            let PaneCardKind::Observation { render, .. } = &card.kind else {
                return None;
            };
            belongs_to_world(card, world).then(|| {
                Line::from(vec![
                    Span::styled("Codex · window ", super::render::muted_style()),
                    Span::raw(format!("“{}” · {}", render.window_name, pane_status(card))),
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
        lines
    }
}

fn bounded_pane_lines(lines: &[Line<'static>], maximum: usize) -> Vec<Line<'static>> {
    if lines.len() <= maximum {
        return lines.to_vec();
    }
    if maximum == 0 {
        return Vec::new();
    }
    let shown = maximum - 1;
    let hidden = lines.len() - shown;
    let mut visible = lines[..shown].to_vec();
    visible.push(Line::styled(
        format!("Codex +{hidden} more panes"),
        super::render::muted_style(),
    ));
    visible
}

pub(super) fn action_lines(world: &ShellWorld) -> Vec<Line<'static>> {
    let actions = match &world.action_log {
        super::action_log::ActionLog::Loading => {
            return vec![Line::styled("Loading…", super::render::muted_style())]
        }
        super::action_log::ActionLog::Unavailable => {
            return vec![Line::styled("Unavailable", super::render::muted_style())]
        }
        super::action_log::ActionLog::Loaded(actions) if actions.is_empty() => {
            return vec![Line::styled(
                "No recorded actions",
                super::render::muted_style(),
            )]
        }
        super::action_log::ActionLog::Loaded(actions) => actions,
    };
    actions
        .iter()
        .map(|action| {
            let Some((source, description)) = action.split_once(": ") else {
                return Line::from(action.clone());
            };
            Line::from(vec![
                Span::styled(format!("{source}: "), super::render::muted_style()),
                Span::raw(description.to_owned()),
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
    live_details: &[Line<'static>],
    action_history: Option<&[Line<'static>]>,
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
    if let Some(action_history) = action_history {
        let maximum_live_rows = usize::from(inner.height)
            .saturating_sub(lines.len())
            .saturating_sub(action_history.len())
            .saturating_sub(1)
            .min(MAX_PANE_ROWS);
        lines.extend(bounded_pane_lines(live_details, maximum_live_rows));
        let separator_y = inner
            .y
            .saturating_add(u16::try_from(lines.len()).unwrap_or(u16::MAX))
            .min(inner.bottom().saturating_sub(1));
        Paragraph::new(lines).render(
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                separator_y.saturating_sub(inner.y),
            ),
            buffer,
        );
        draw_action_history(buffer, area, inner, separator_y, action_history);
        return;
    }
    lines.extend_from_slice(live_details);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    Paragraph::new(lines).render(rows[0], buffer);
    Paragraph::new(footer)
        .style(super::render::muted_style())
        .render(rows[1], buffer);
}

fn draw_action_history(
    buffer: &mut Buffer,
    area: Rect,
    inner: Rect,
    separator_y: u16,
    lines: &[Line<'static>],
) {
    let style = super::render::muted_style();
    Block::new()
        .borders(Borders::TOP)
        .border_style(style)
        .title(Span::styled(
            "── Action history ",
            style.add_modifier(Modifier::BOLD),
        ))
        .render(
            Rect::new(
                area.x,
                separator_y,
                area.width,
                area.bottom().saturating_sub(1).saturating_sub(separator_y),
            ),
            buffer,
        );
    if area.width > 1 {
        buffer[(area.x, separator_y)]
            .set_symbol("├")
            .set_style(style);
        buffer[(area.right().saturating_sub(1), separator_y)]
            .set_symbol("┤")
            .set_style(style);
    }
    let content = Rect::new(
        inner.x,
        separator_y.saturating_add(1),
        inner.width,
        inner.bottom().saturating_sub(separator_y.saturating_add(1)),
    );
    Paragraph::new(lines.to_vec())
        .style(style)
        .render(content, buffer);
    for (index, line) in lines.iter().take(usize::from(content.height)).enumerate() {
        if line.width() > usize::from(content.width) && content.width > 0 {
            let row = content
                .y
                .saturating_add(u16::try_from(index).unwrap_or(u16::MAX));
            buffer[(content.right().saturating_sub(1), row)]
                .set_symbol("…")
                .set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_card_buffer(world: &ShellWorld, cards: &[PaneCard], width: u16) -> Buffer {
        let area = Rect::new(0, 0, width, super::super::control::WORLD_CARD_HEIGHT);
        let mut buffer = Buffer::empty(area);
        let (icon, color, status) = status(world, Liveness::Current);
        let live = pane_lines(world, cards);
        let history = action_lines(world);
        draw(
            &mut buffer,
            area,
            icon,
            color,
            &status,
            &world.name,
            &world.resources,
            (world.detail != "-").then_some(world.detail.as_str()),
            &live,
            Some(&history),
            false,
            "",
            true,
        );
        buffer
    }

    fn rendered_card(world: &ShellWorld, cards: &[PaneCard], width: u16) -> String {
        let buffer = rendered_card_buffer(world, cards, width);
        buffer
            .content()
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn world_with_actions() -> ShellWorld {
        let mut world = ShellWorld::test("ars.clever-turtle", 1);
        world.resources = "2 CPU · 4G · 6G/32G disk".into();
        world.action_log = super::super::action_log::ActionLog::Loaded(vec![
            "wtg: opened PR #250 for wt/world-card-action-log · github.com/lucasavila00/wt".into(),
            "Git: pushed wt/world-card-action-log to github.com/lucasavila00/wt".into(),
            "wtg: checked CI · github.com/lucasavila00/wt".into(),
            "Git: fetched from github.com/lucasavila00/wt".into(),
            "wtg: commented on PR #250 · github.com/lucasavila00/wt".into(),
        ]);
        world
    }

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
        let window_index = pane_id.trim_start_matches('%').parse().unwrap();
        let window_name = match window_index {
            1 => "codex".to_owned(),
            2 => "make".to_owned(),
            _ => format!("window-{window_index}"),
        };
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
            classified_at_unix_ms: now,
            kind: PaneCardKind::Observation {
                world_name: world.world_name.to_string(),
                changed_at_unix_ms,
                cwd: "/home/wt".into(),
                git_branch: None,
                render: wt_control_protocol::PaneRender {
                    window_index,
                    window_name,
                    frame: wt_control_protocol::PaneFrame {
                        rows: 1,
                        columns: 1,
                        cells: vec![wt_control_protocol::PaneCell {
                            text: "C".into(),
                            foreground: wt_control_protocol::PaneColor::Default,
                            background: wt_control_protocol::PaneColor::Default,
                            bold: false,
                            italic: false,
                            underlined: false,
                            inverse: false,
                        }],
                    },
                },
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
        Codex · window “codex” · CHANGING
        Codex · window “make” · STATIC
        "###
        );
    }

    #[test]
    fn formats_action_and_empty_pane_lines() {
        let mut world = ShellWorld::test("ars.dev", 1);
        world.action_log = super::super::action_log::ActionLog::Loaded(vec![
            "wtg: opened PR #42 for wt/topic · github.com/owner/write".into(),
            "Git: pushed wt/topic to github.com/owner/write".into(),
        ]);

        insta::assert_snapshot!(
            action_lines(&world)
                .into_iter()
                .chain(pane_lines(&world, &[]))
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            @r###"
        wtg: opened PR #42 for wt/topic · github.com/owner/write
        Git: pushed wt/topic to github.com/owner/write
        Codex no observed Byobu panes
        "###
        );
    }

    #[test]
    fn snapshots_a_populated_action_history_compartment() {
        let world = world_with_actions();
        let now = now_unix_ms();
        let panes = vec![observation(&world, "%1", now)];

        insta::assert_snapshot!(
            "world_card_action_history_populated",
            rendered_card(&world, &panes, 76)
        );
    }

    #[test]
    fn snapshots_dense_live_and_history_content_at_narrow_width() {
        let world = world_with_actions();
        let now = now_unix_ms();
        let panes = (1..=8)
            .map(|index| observation(&world, &format!("%{index}"), now))
            .collect::<Vec<_>>();

        insta::assert_snapshot!(
            "world_card_action_history_dense_narrow",
            rendered_card(&world, &panes, 52)
        );
    }

    #[test]
    fn snapshots_empty_action_history_separately_from_live_state() {
        let mut world = ShellWorld::test("ars.quiet-panda", 1);
        world.action_log = super::super::action_log::ActionLog::Loaded(Vec::new());

        insta::assert_snapshot!(
            "world_card_action_history_empty",
            rendered_card(&world, &[], 52)
        );
    }

    #[test]
    fn snapshots_action_history_loading_and_unavailable_states() {
        let loading = ShellWorld::test("ars.loading-panda", 1);
        let mut unavailable = ShellWorld::test("ars.offline-panda", 2);
        unavailable.action_log = super::super::action_log::ActionLog::Unavailable;

        insta::assert_snapshot!(
            "world_card_action_history_loading",
            rendered_card(&loading, &[], 52)
        );
        insta::assert_snapshot!(
            "world_card_action_history_unavailable",
            rendered_card(&unavailable, &[], 52)
        );
    }

    #[test]
    fn snapshots_world_error_detail_above_action_history() {
        let mut world = world_with_actions();
        world.status = wt_control_protocol::WorldStatus::Error;
        world.detail = "SSH readiness failed; run `wt rm ars.clever-turtle`".into();

        insta::assert_snapshot!(
            "world_card_action_history_with_error_detail",
            rendered_card(&world, &[], 76)
        );
    }

    #[test]
    fn action_history_is_dimmed_while_live_status_remains_normal() {
        let world = world_with_actions();
        let now = now_unix_ms();
        let buffer = rendered_card_buffer(&world, &[observation(&world, "%1", now)], 76);

        assert!(!buffer[(7, 3)].modifier.contains(Modifier::DIM));
        assert!(buffer[(6, 5)].modifier.contains(Modifier::DIM));
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
    fn marks_stale_panes_as_a_lost_connection() {
        let world = ShellWorld::test("ars.dev", 1);
        let mut stale_pane = observation(&world, "%1", 0);
        stale_pane.observed_at_unix_ms = Some(0);

        assert!(has_lost_connection(&world, &[stale_pane]));
        insta::assert_snapshot!(
            status(&world, Liveness::ConnectionLost).2,
            @"CONNECTION LOST · NO PANE UPDATE"
        );
    }

    #[test]
    fn bounds_long_pane_lists_with_an_overflow_row() {
        let world = ShellWorld::test("ars.dev", 1);
        let now = now_unix_ms();
        let panes = (1..=8)
            .map(|index| observation(&world, &format!("%{index}"), now))
            .collect::<Vec<_>>();

        insta::assert_snapshot!(
            bounded_pane_lines(&pane_lines(&world, &panes), MAX_PANE_ROWS)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            @r###"
        Codex · window “codex” · CHANGING
        Codex · window “make” · CHANGING
        Codex · window “window-3” · CHANGING
        Codex · window “window-4” · CHANGING
        Codex · window “window-5” · CHANGING
        Codex +3 more panes
        "###
        );
    }
}
