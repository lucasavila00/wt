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
const WIDE_BREAKPOINT: u16 = 400;
const WIDE_COLUMNS: usize = 4;
const WIDE_ROWS: usize = 4;
const GAP: u16 = 1;

pub(super) fn columns(area: Rect) -> usize {
    if area.width >= WIDE_BREAKPOINT {
        WIDE_COLUMNS
    } else {
        1
    }
}

pub(super) fn visible(area: Rect) -> usize {
    if columns(area) == WIDE_COLUMNS {
        WIDE_COLUMNS * WIDE_ROWS
    } else {
        let (body, _) = control_content_areas(area);
        usize::from(body.inner(Margin::new(1, 1)).height.div_ceil(CARD_HEIGHT))
    }
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
    if columns(area) == 1 {
        return Some((CARD_HEIGHT.min(viewport.height), viewport.width));
    }
    Some((
        (viewport.height.saturating_sub(GAP * 3) / 4).max(3),
        (viewport.width.saturating_sub(GAP * 3) / 4).max(3),
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
    let content_rows = count.div_ceil(columns);
    let viewport_rows = visible.div_ceil(columns).max(1);
    let viewport_right = viewport
        .right()
        .saturating_sub(u16::from(content_rows > viewport_rows));
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
                    width.min(viewport_right.saturating_sub(x)),
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
    let columns = columns(frame.area());
    let content_rows = cards.len().div_ceil(columns);
    let viewport_rows = visible(frame.area()).div_ceil(columns).max(1);
    super::scrollbar::render(
        frame,
        super::scrollbar::area(frame.area()),
        content_rows,
        viewport_rows,
        state.codex_offset() / columns,
        muted_style(),
    );
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
    fn wide_layout_is_four_by_four_at_the_breakpoint() {
        let area = Rect::new(0, 0, 400, 40);
        let rects = card_rects(area, 0, 16);
        assert_eq!(columns(area), 4);
        assert_eq!(visible(area), 16);
        assert_eq!(rects.len(), 16);
        assert_eq!(rects[0].1.y, rects[3].1.y);
        assert!(rects[4].1.y > rects[0].1.y);
        for (index, (_, rect)) in rects.iter().enumerate() {
            assert!(rect.width >= 3 && rect.height >= 3);
            assert!(rect.right() <= area.right() && rect.bottom() <= area.bottom());
            assert!(rects[..index]
                .iter()
                .all(|(_, other)| !rect.intersects(*other)));
        }
    }

    #[test]
    fn narrow_layout_uses_one_card_per_row() {
        let area = Rect::new(0, 0, 399, 40);
        let rects = card_rects(area, 0, 3);
        assert_eq!(columns(area), 1);
        assert_eq!(rects.len(), 3);
        assert!(rects.windows(2).all(|pair| pair[1].1.y > pair[0].1.y));
    }

    #[test]
    fn live_cards_reserve_a_scrollbar_column_only_when_overflowing() {
        let area = Rect::new(0, 0, 399, 40);
        let fitting = card_rects(area, 0, 4);
        let overflowing = card_rects(area, 0, 5);

        assert_eq!(overflowing[0].1.width + 1, fitting[0].1.width);
    }
}
