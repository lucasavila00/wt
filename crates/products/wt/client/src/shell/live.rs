use super::control::control_content_areas;
use super::model::ShellModel;
use super::render::{card_title, muted_style, selected_card_border_style};
use super::terminal_view::TerminalView;
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub(super) const CARD_HEIGHT: u16 = 12;
const COLUMNS: usize = 2;
const GAP: u16 = 1;

pub(super) fn columns(_area: Rect) -> usize {
    COLUMNS
}

pub(super) fn visible(area: Rect) -> usize {
    let (body, _) = control_content_areas(area);
    usize::from(
        body.inner(Margin::new(1, 1))
            .height
            .div_ceil(CARD_HEIGHT + GAP),
    ) * COLUMNS
}

pub(super) fn preview_size(area: Rect) -> (u16, u16) {
    card_size(area).map_or((1, 1), |(height, width)| {
        (
            height.saturating_sub(2).max(1),
            width.saturating_sub(2).max(1),
        )
    })
}

fn card_size(area: Rect) -> Option<(u16, u16)> {
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    if viewport.is_empty() {
        return None;
    }
    Some((
        CARD_HEIGHT.min(viewport.height),
        (viewport.width.saturating_sub(GAP) / 2).max(1),
    ))
}

pub(super) fn card_rects(area: Rect, offset: usize, count: usize) -> Vec<(usize, Rect)> {
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    if viewport.is_empty() {
        return Vec::new();
    }
    let columns = columns(area);
    let visible = visible(area);
    let (height, width) = card_size(area).unwrap_or((1, 1));
    (offset..count.min(offset.saturating_add(visible)))
        .enumerate()
        .map(|(row, index)| {
            let grid_row = row / columns;
            let grid_column = row % columns;
            let y = viewport.y + u16::try_from(grid_row).unwrap_or(u16::MAX) * (height + GAP);
            let x = viewport.x + u16::try_from(grid_column).unwrap_or(u16::MAX) * (width + GAP);
            (
                index,
                Rect::new(
                    x,
                    y,
                    width.min(viewport.right().saturating_sub(x)),
                    height.min(viewport.bottom().saturating_sub(y)),
                ),
            )
        })
        .collect()
}

pub(super) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    screens: &[&vt100::Screen],
    live_focus: &super::live_focus::LiveFocus,
    model: &ShellModel,
) {
    let state = model.control();
    let block = Block::new()
        .borders(Borders::ALL)
        .title("Live sessions · Experimental");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let cards = state.live_codex();
    if cards.is_empty() {
        frame.render_widget(
            Paragraph::new("No live Codex sessions\nStart Codex in a world to see its pane here")
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }
    for (index, rect) in card_rects(frame.area(), state.codex_offset(), cards.len()) {
        let card = cards[index];
        let (title, title_color) = card_title(card);
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
        frame.render_widget(block, rect);
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
            frame.render_widget(TerminalView(screen), viewport);
        } else {
            frame.render_widget(
                Paragraph::new(card.disabled_reason().unwrap_or("preview unavailable"))
                    .alignment(Alignment::Center)
                    .style(muted_style()),
                viewport,
            );
        }
        if let Some(warning) = live_focus.warning(card, state.codex()) {
            let warning_area = Rect::new(
                viewport.x,
                viewport.bottom().saturating_sub(1),
                viewport.width,
                1.min(viewport.height),
            );
            frame.render_widget(
                Paragraph::new(warning)
                    .alignment(Alignment::Center)
                    .style(Style::new().add_modifier(Modifier::REVERSED)),
                warning_area,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_uses_two_equal_columns() {
        let area = Rect::new(0, 0, 100, 30);
        let rects = card_rects(area, 0, 6);
        assert_eq!(columns(area), 2);
        assert_eq!(visible(area), 6);
        assert_eq!(rects.len(), 6);
        assert_eq!(rects[0].1.y, rects[1].1.y);
        assert!(rects[2].1.y > rects[0].1.y);
        assert_eq!(rects[0].1.width, rects[1].1.width);
        for (index, (_, rect)) in rects.iter().enumerate() {
            assert!(rect.width >= 1 && rect.height >= 1);
            assert!(rect.right() <= area.right() && rect.bottom() <= area.bottom());
            assert!(rects[..index]
                .iter()
                .all(|(_, other)| !rect.intersects(*other)));
        }
    }

    #[test]
    fn offset_preserves_two_column_rows() {
        let area = Rect::new(0, 0, 100, 30);
        let rects = card_rects(area, 2, 6);
        assert_eq!(rects[0].0, 2);
        assert_eq!(rects[0].1.y, rects[1].1.y);
    }
}
