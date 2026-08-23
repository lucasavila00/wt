use super::control::{card_grid_with_gap, CARD_COLUMNS};
use super::model::ShellModel;
use super::render::{card_title, muted_style, selected_card_border_style};
use super::terminal_view::TerminalView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

pub(super) const CARD_HEIGHT: u16 = 16;
pub(super) const CARD_GAP: u16 = 0;
// The card viewport clips this row, leaving Byobu's status bar out of the preview.
const BYOBU_STATUS_ROWS: u16 = 1;

pub(super) fn columns(_area: Rect) -> usize {
    CARD_COLUMNS
}

pub(super) fn preview_size(area: Rect, count: usize) -> (u16, u16) {
    let (height, width) = card_grid_with_gap(area, 0, count, CARD_HEIGHT, CARD_GAP).card_size();
    (
        height
            .saturating_sub(2)
            .saturating_add(BYOBU_STATUS_ROWS)
            .max(1),
        width.saturating_sub(2).max(1),
    )
}

pub(super) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    screens: &[&vt100::Screen],
    live_focus: &super::live_focus::LiveFocus,
    model: &ShellModel,
) {
    let state = model.control();
    let cards = state.live_codex();
    if cards.is_empty() {
        frame.render_widget(
            Paragraph::new("No live Codex sessions\nStart Codex in a world to see its pane here")
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    let grid = card_grid_with_gap(
        frame.area(),
        state.codex_scroll(),
        cards.len(),
        CARD_HEIGHT,
        CARD_GAP,
    );
    super::scrollbar::render(frame, grid, muted_style());
    for placement in grid.cards() {
        let card = cards[placement.index];
        grid.render_card(frame, placement, |rect, buffer| {
            draw_card(buffer, rect, card, screens, live_focus, model)
        });
    }
}

fn draw_card(
    buffer: &mut Buffer,
    rect: Rect,
    card: &super::control::CodexCard,
    screens: &[&vt100::Screen],
    live_focus: &super::live_focus::LiveFocus,
    model: &ShellModel,
) {
    let state = model.control();
    let (title, title_color) = live_card_title(card, live_focus.is_stuck(card));
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_style(selected_card_border_style(
            state.selected() == Some(&card.identity),
        ))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(title_color).add_modifier(Modifier::BOLD),
        ));
    if let Some(repository) = card_repository(card) {
        block = block.title_bottom(
            Line::from(vec![Span::raw(" "), Span::raw(repository), Span::raw(" ")]).right_aligned(),
        );
    }
    let viewport = block.inner(rect);
    block.render(rect, buffer);
    let screen = match &card.kind {
        super::control::CodexCardKind::Observation { world_id, .. } => model
            .worlds()
            .iter()
            .position(|world| {
                world.identity.context == card.context && world.identity.id == *world_id
            })
            .and_then(|index| screens.get(index).copied()),
        super::control::CodexCardKind::RolloutOnly
        | super::control::CodexCardKind::ContextError { .. } => None,
    };
    if let Some(screen) = screen {
        TerminalView(screen).render(viewport, buffer);
    } else {
        Paragraph::new(card.disabled_reason().unwrap_or("preview unavailable"))
            .alignment(Alignment::Center)
            .style(muted_style())
            .render(viewport, buffer);
    }
    if let Some(warning) = live_focus.warning(card, state.codex()) {
        let warning_area = Rect::new(
            viewport.x,
            viewport.bottom().saturating_sub(1),
            viewport.width,
            1.min(viewport.height),
        );
        Paragraph::new(warning)
            .alignment(Alignment::Center)
            .style(Style::new().add_modifier(Modifier::REVERSED))
            .render(warning_area, buffer);
    }
}

fn live_card_title(card: &super::control::CodexCard, stuck: bool) -> (String, Color) {
    let (title, title_color) = if stuck {
        ("󰚩 POSSIBLY STUCK".into(), Color::Yellow)
    } else {
        card_title(card)
    };
    let title = match &card.kind {
        super::control::CodexCardKind::Observation {
            world_name,
            git_branch,
            ..
        } => git_branch.as_ref().map_or_else(
            || format!("{title} · {}.{world_name}", card.context),
            |branch| format!("{title} · {}.{world_name} · {branch}", card.context),
        ),
        super::control::CodexCardKind::RolloutOnly
        | super::control::CodexCardKind::ContextError { .. } => title,
    };
    (title, title_color)
}

fn card_repository(card: &super::control::CodexCard) -> Option<String> {
    let super::control::CodexCardKind::Observation {
        repository_root,
        repository_url,
        ..
    } = &card.kind
    else {
        return None;
    };
    repository_url
        .as_deref()
        .and_then(repository_identifier)
        .or_else(|| {
            repository_root
                .as_deref()
                .and_then(|root| std::path::Path::new(root).file_name()?.to_str())
                .map(str::to_owned)
        })
}

fn repository_identifier(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/').trim_end_matches(".git");
    let remote = url.rsplit('@').next()?;
    let (host, path) = if let Some(remote) = remote.strip_prefix("https://") {
        remote.split_once('/')?
    } else if let Some(remote) = remote.strip_prefix("http://") {
        remote.split_once('/')?
    } else {
        remote.split_once(':')?
    };
    let mut components = path.rsplit('/');
    let repository = components.next()?;
    let owner = components.next()?;
    if host.is_empty() || owner.is_empty() || repository.is_empty() {
        return None;
    }
    let provider = match host {
        "github.com" => "github",
        "gitlab.com" => "gitlab",
        host => host,
    };
    Some(format!("{provider}:{owner}/{repository}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use wt_control_protocol::{ByobuTarget, CodexSessionState};

    fn working_card() -> super::super::control::CodexCard {
        let session_id = Uuid::from_u128(1);
        super::super::control::CodexCard {
            identity: super::super::control::CodexCardIdentity::Observation {
                context: "local".into(),
                session_id,
                world_id: Uuid::from_u128(2),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
            },
            context: "local".into(),
            session_id: Some(session_id),
            timestamp: Some(1),
            latest_user_message: None,
            kind: super::super::control::CodexCardKind::Observation {
                world_id: Uuid::from_u128(2),
                world_name: "world".into(),
                cwd: "/home/wt".into(),
                repository_root: None,
                repository_url: None,
                git_branch: Some("wt/change".into()),
                git_context_health: None,
                state: CodexSessionState::Working,
                is_compacting: false,
                session_start_source: None,
                target: ByobuTarget {
                    tmux_session: "wt-host".into(),
                    pane_id: "%1".into(),
                },
            },
        }
    }

    #[test]
    fn stuck_title_uses_attention_color_and_keeps_location() {
        let (title, color) = live_card_title(&working_card(), true);

        assert_eq!(title, "󰚩 POSSIBLY STUCK · local.world · wt/change");
        assert_eq!(color, Color::Yellow);
    }

    #[test]
    fn repository_identifier_keeps_the_owner_and_name() {
        assert_eq!(
            repository_identifier("git@github.com:lucasavila00/wt.git"),
            Some("github:lucasavila00/wt".into())
        );
        assert_eq!(
            repository_identifier("https://github.com/lucasavila00/wt"),
            Some("github:lucasavila00/wt".into())
        );
        assert_eq!(
            repository_identifier("git@git.example.com:platform/wt.git"),
            Some("git.example.com:platform/wt".into())
        );
    }

    #[test]
    fn layout_uses_two_equal_columns() {
        let area = Rect::new(0, 0, 100, 30);
        let grid = card_grid_with_gap(area, 0, 6, CARD_HEIGHT, CARD_GAP);
        let rects = (0..4)
            .map(|index| grid.card_rect(index).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(columns(area), 2);
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0].y, rects[1].y);
        assert_eq!(rects[1].x, rects[0].right());
        assert_eq!(rects[2].y, rects[0].bottom());
        assert_eq!(rects[0].width, rects[1].width);
        for (index, rect) in rects.iter().enumerate() {
            assert!(rect.width >= 1 && rect.height >= 1);
            assert!(rect.right() <= area.right() && rect.bottom() <= area.bottom());
            assert!(rects[..index].iter().all(|other| !rect.intersects(*other)));
        }
    }

    #[test]
    fn scroll_moves_the_canvas_by_terminal_rows() {
        let area = Rect::new(0, 0, 100, 30);
        let first = card_grid_with_gap(area, 0, 8, CARD_HEIGHT, CARD_GAP)
            .card_rect(2)
            .unwrap();
        let scrolled = card_grid_with_gap(area, 2, 8, CARD_HEIGHT, CARD_GAP)
            .card_rect(2)
            .unwrap();
        assert_eq!(scrolled.y, first.y - 2);
    }

    #[test]
    fn live_cards_reserve_a_scrollbar_column_only_when_overflowing() {
        let area = Rect::new(0, 0, 100, 34);
        let fitting = card_grid_with_gap(area, 0, 4, CARD_HEIGHT, CARD_GAP);
        let overflowing = card_grid_with_gap(area, 0, 6, CARD_HEIGHT, CARD_GAP);

        assert!(overflowing.viewport.right() <= super::super::scrollbar::area(area).x);
        assert!(overflowing.viewport.right() < fitting.viewport.right());
    }
}
