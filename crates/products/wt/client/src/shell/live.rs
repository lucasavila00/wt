use super::control::{card_grid_with_gap, PaneCard, PaneCardKind, CARD_COLUMNS};
use super::model::ShellModel;
use super::render::{card_title, muted_style, selected_card_border_style};
use super::terminal_view::PaneFrameView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

pub(super) const CARD_HEIGHT: u16 = 18;
pub(super) const CARD_GAP: u16 = 0;
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

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &ShellModel) {
    let state = model.control();
    if state.panes().is_empty() {
        frame.render_widget(
            Paragraph::new("No live Codex panes\nStart Codex in a world to see its screen here")
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    let grid = card_grid_with_gap(
        frame.area(),
        state.pane_scroll(),
        state.panes().len(),
        CARD_HEIGHT,
        CARD_GAP,
    );
    super::scrollbar::render(frame, grid, muted_style());
    for placement in grid.cards() {
        let card = &state.panes()[placement.index];
        grid.render_card(frame, placement, |rect, buffer| {
            draw_card(buffer, rect, card, state.selected() == Some(&card.identity))
        });
    }
}

fn draw_card(buffer: &mut Buffer, rect: Rect, card: &PaneCard, selected: bool) {
    let (status, color) = card_title(card);
    let title = match &card.kind {
        PaneCardKind::Observation { world_name, .. } => {
            format!("{status} · {}.{world_name}", card.context)
        }
        PaneCardKind::ContextError => status,
    };
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_style(selected_card_border_style(selected))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
    if let Some(location) = card.location() {
        block = block.title_bottom(
            Line::from(format!(" {location} "))
                .alignment(Alignment::Right)
                .style(muted_style()),
        );
    }
    let viewport = block.inner(rect);
    block.render(rect, buffer);
    if let Some(frame) = card.frame() {
        PaneFrameView(frame).render(viewport, buffer);
    } else {
        Paragraph::new(card.disabled_reason().unwrap_or("preview unavailable"))
            .alignment(Alignment::Center)
            .style(muted_style())
            .render(viewport, buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::control::PaneCardIdentity;

    #[test]
    fn live_uses_two_equal_columns() {
        assert_eq!(columns(Rect::new(0, 0, 100, 30)), 2);
    }

    #[test]
    fn preview_size_matches_the_live_card_viewport() {
        assert_eq!(preview_size(Rect::new(0, 0, 100, 30), 1), (17, 45));
    }

    #[test]
    fn live_card_insets_the_cwd_and_git_branch_in_its_footer() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap();
        let card = PaneCard {
            identity: PaneCardIdentity::Observation {
                context: "ars".into(),
                world_id: uuid::Uuid::nil().into(),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
            },
            context: "ars".into(),
            created_at_unix_ms: Some(now),
            observed_at_unix_ms: Some(now),
            kind: PaneCardKind::Observation {
                world_name: "dev".into(),
                changed_at_unix_ms: now,
                cwd: "/home/wt/wt".into(),
                git_branch: Some("wt/live-pane-cwd".into()),
                frame: None,
            },
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, 52, 4));

        draw_card(&mut buffer, Rect::new(0, 0, 52, 4), &card, false);
        let footer = buffer
            .content()
            .chunks(52)
            .last()
            .unwrap()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("");

        insta::assert_snapshot!(footer, @"└────────────────── /home/wt/wt · wt/live-pane-cwd ┘");
    }
}
