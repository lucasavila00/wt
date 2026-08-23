use super::Activity;
use ratatui::layout::{Constraint, Layout, Margin, Rect};

pub(in crate::shell) const ACTIVITY_BAR_WIDTH: u16 = 5;
pub(in crate::shell) const ACTIVITY_BUTTON_HEIGHT: u16 = 3;
pub(in crate::shell) const CODEX_CARD_HEIGHT: u16 = 8;
pub(in crate::shell) const WORLD_CARD_HEIGHT: u16 = 10;
pub(in crate::shell) const CARD_COLUMNS: usize = 2;
pub(in crate::shell) const CARD_GAP: u16 = 1;

pub(in crate::shell) fn control_areas(area: Rect) -> (Rect, Rect) {
    let columns = Layout::horizontal([Constraint::Length(ACTIVITY_BAR_WIDTH), Constraint::Min(0)])
        .split(area);
    (columns[0], columns[1])
}

pub(in crate::shell) fn control_content_areas(area: Rect) -> (Rect, Rect) {
    let (_, content) = control_areas(area);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(content);
    (rows[0], rows[1])
}

pub(in crate::shell) fn codex_card_rects(
    area: Rect,
    offset: usize,
    count: usize,
) -> Vec<(usize, Rect)> {
    card_grid_rects(area, offset, count, CODEX_CARD_HEIGHT, CARD_GAP)
}

pub(in crate::shell) fn card_grid_rects(
    area: Rect,
    offset: usize,
    count: usize,
    card_height: u16,
    card_gap: u16,
) -> Vec<(usize, Rect)> {
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    if viewport.is_empty() {
        return Vec::new();
    }
    let visible = card_grid_visible(area, card_height, card_gap);
    let viewport_width = viewport.width.saturating_sub(u16::from(count > visible));
    let viewport_right = viewport.x.saturating_add(viewport_width);
    let width = (viewport_width.saturating_sub(card_gap) / 2).max(1);
    (offset..count.min(offset.saturating_add(visible)))
        .enumerate()
        .map(|(position, index)| {
            let row = position / CARD_COLUMNS;
            let column = position % CARD_COLUMNS;
            let x = viewport.x + u16::try_from(column).unwrap_or(u16::MAX) * (width + card_gap);
            let y = viewport.y + u16::try_from(row).unwrap_or(u16::MAX) * (card_height + card_gap);
            (
                index,
                Rect::new(
                    x,
                    y,
                    width.min(viewport_right.saturating_sub(x)),
                    card_height.min(viewport.bottom().saturating_sub(y)),
                ),
            )
        })
        .collect()
}

pub(in crate::shell) fn world_card_rects(
    area: Rect,
    selected: usize,
    count: usize,
) -> Vec<(usize, Rect)> {
    let visible = card_grid_visible(area, WORLD_CARD_HEIGHT, CARD_GAP).max(1);
    let offset = selected / visible * visible;
    card_grid_rects(area, offset, count, WORLD_CARD_HEIGHT, CARD_GAP)
}

pub(in crate::shell) fn card_grid_visible(area: Rect, card_height: u16, card_gap: u16) -> usize {
    let (body, _) = control_content_areas(area);
    usize::from(
        body.inner(Margin::new(1, 1))
            .height
            .div_ceil(card_height + card_gap),
    ) * CARD_COLUMNS
}

pub(in crate::shell) fn world_card_at_position(
    area: Rect,
    selected: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    world_card_rects(area, selected, count)
        .into_iter()
        .find(|(_, rect)| rect.contains((column, row).into()))
        .map(|(index, _)| index)
}

pub(in crate::shell) fn codex_visible_cards(area: Rect, activity: Activity) -> usize {
    if activity == Activity::Live {
        return super::super::live::visible(area);
    }
    card_grid_visible(area, CODEX_CARD_HEIGHT, CARD_GAP)
}

pub(in crate::shell) fn session_card_at_position(
    area: Rect,
    activity: Activity,
    offset: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    let rects = if activity == Activity::Live {
        super::super::live::card_rects(area, offset, count)
    } else {
        codex_card_rects(area, offset, count)
    };
    rects
        .into_iter()
        .find(|(_, rect)| rect.contains((column, row).into()))
        .map(|(index, _)| index)
}
