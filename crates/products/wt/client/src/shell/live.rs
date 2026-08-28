use super::control::{card_grid_with_gap, PaneCard, PaneCardKind, CARD_COLUMNS};
use super::model::ShellModel;
use super::render::{card_title, muted_style, selected_card_border_style};
use super::terminal_view::PaneFrameView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

pub(super) const CARD_HEIGHT: u16 = 18;
pub(super) const CARD_GAP: u16 = 0;

pub(super) fn columns(_area: Rect) -> usize {
    CARD_COLUMNS
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
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(selected_card_border_style(selected))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
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

    #[test]
    fn live_uses_two_equal_columns() {
        assert_eq!(columns(Rect::new(0, 0, 100, 30)), 2);
    }
}
