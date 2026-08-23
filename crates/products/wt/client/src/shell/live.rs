use super::control::{card_grid_with_gap, CARD_COLUMNS};
use super::model::ShellModel;
use super::render::{card_title, muted_style, selected_card_border_style};
use super::terminal_view::TerminalView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

pub(super) const CARD_HEIGHT: u16 = 14;
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
    let (title, title_color) = card_title(card);
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
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(selected_card_border_style(
            state.selected() == Some(&card.identity),
        ))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(title_color).add_modifier(Modifier::BOLD),
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let area = Rect::new(0, 0, 100, 30);
        let fitting = card_grid_with_gap(area, 0, 4, CARD_HEIGHT, CARD_GAP);
        let overflowing = card_grid_with_gap(area, 0, 6, CARD_HEIGHT, CARD_GAP);

        assert!(overflowing.viewport.right() <= super::super::scrollbar::area(area).x);
        assert!(overflowing.viewport.right() < fitting.viewport.right());
    }
}
