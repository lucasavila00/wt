use super::control::{card_grid_with_gap, PaneCard, PaneCardKind, CARD_COLUMNS};
use super::model::ShellModel;
use super::render::{card_title, muted_style, selected_card_border_style};
use super::terminal_view::TerminalView;
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

pub(super) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    screens: &[&vt100::Screen],
    model: &ShellModel,
) {
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
            draw_card(
                buffer,
                rect,
                card,
                screens,
                model,
                state.selected() == Some(&card.identity),
            )
        });
    }
}

fn draw_card(
    buffer: &mut Buffer,
    rect: Rect,
    card: &PaneCard,
    screens: &[&vt100::Screen],
    model: &ShellModel,
    selected: bool,
) {
    let (status, color) = card_title(card);
    let title = match &card.kind {
        PaneCardKind::Observation { world_name, .. } => {
            format!("{status} · {}.{world_name}", card.context)
        }
        PaneCardKind::ContextError { .. } => status,
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
    let screen = match &card.kind {
        PaneCardKind::Observation { .. } => card
            .world_id()
            .and_then(|world_id| {
                model.worlds().iter().position(|world| {
                    world.identity.context == card.context && world.identity.world_id == world_id
                })
            })
            .and_then(|index| screens.get(index).copied()),
        PaneCardKind::ContextError { .. } => None,
    };
    if let Some(screen) = screen {
        TerminalView(screen).render(viewport, buffer);
    } else {
        Paragraph::new(card.disabled_reason().unwrap_or("preview unavailable"))
            .alignment(Alignment::Center)
            .style(muted_style())
            .render(viewport, buffer);
    }
    let footer = Rect::new(
        viewport.x,
        viewport.bottom().saturating_sub(1),
        viewport.width,
        viewport.height.min(1),
    );
    Paragraph::new(Line::from("Click or Enter for Codex details"))
        .alignment(Alignment::Center)
        .style(muted_style())
        .render(footer, buffer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_two_equal_columns() {
        let (rows, preview_columns) = preview_size(Rect::new(0, 0, 100, 30), 1);

        assert_eq!(rows, 17);
        assert_eq!(preview_columns, 45);
        assert_eq!(columns(Rect::new(0, 0, 100, 30)), 2);
    }
}
