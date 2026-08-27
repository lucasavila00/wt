use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    Frame,
};

pub(in crate::shell) const ACTIVITY_BAR_WIDTH: u16 = 5;
pub(in crate::shell) const ACTIVITY_BUTTON_HEIGHT: u16 = 3;
pub(in crate::shell) const WORLD_CARD_HEIGHT: u16 = 10;
pub(in crate::shell) const CARD_COLUMNS: usize = 2;
pub(in crate::shell) const CARD_GAP: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::shell) struct CardGrid {
    pub(in crate::shell) viewport: Rect,
    pub(in crate::shell) content_height: usize,
    pub(in crate::shell) scroll: usize,
    card_height: u16,
    card_width: u16,
    card_gap: u16,
    count: usize,
}

impl CardGrid {
    pub(in crate::shell) fn card_size(self) -> (u16, u16) {
        (self.card_height, self.card_width)
    }

    pub(in crate::shell) fn maximum_scroll(self) -> usize {
        self.content_height
            .saturating_sub(usize::from(self.viewport.height))
    }

    pub(in crate::shell) fn cards(self) -> impl Iterator<Item = CardPlacement> {
        (0..self.count).filter_map(move |index| {
            let row = index / CARD_COLUMNS;
            let column = index % CARD_COLUMNS;
            let content_y = row.saturating_mul(usize::from(self.card_height + self.card_gap));
            let content_bottom = content_y.saturating_add(usize::from(self.card_height));
            let viewport_bottom = self
                .scroll
                .saturating_add(usize::from(self.viewport.height));
            if content_bottom <= self.scroll || content_y >= viewport_bottom {
                return None;
            }
            let x = u16::try_from(column)
                .unwrap_or(u16::MAX)
                .saturating_mul(self.card_width.saturating_add(self.card_gap));
            let width = self.card_width.min(self.viewport.width.saturating_sub(x));
            if width == 0 {
                return None;
            }
            Some(CardPlacement {
                index,
                x,
                content_y,
                width,
                height: self.card_height,
            })
        })
    }

    pub(in crate::shell) fn card_at(self, column: u16, row: u16) -> Option<usize> {
        if !self.viewport.contains(Position::new(column, row)) {
            return None;
        }
        let x = column.saturating_sub(self.viewport.x);
        let content_y =
            usize::from(row.saturating_sub(self.viewport.y)).saturating_add(self.scroll);
        self.cards().find_map(|card| {
            let in_x = x >= card.x && x < card.x.saturating_add(card.width);
            let in_y = content_y >= card.content_y
                && content_y < card.content_y.saturating_add(usize::from(card.height));
            (in_x && in_y).then_some(card.index)
        })
    }

    #[cfg(test)]
    pub(in crate::shell) fn card_rect(self, index: usize) -> Option<Rect> {
        let card = self.cards().find(|card| card.index == index)?;
        let source_y = self.scroll.saturating_sub(card.content_y);
        let target_y = card.content_y.saturating_sub(self.scroll);
        let height = usize::from(card.height)
            .saturating_sub(source_y)
            .min(usize::from(self.viewport.height).saturating_sub(target_y));
        Some(Rect::new(
            self.viewport.x.saturating_add(card.x),
            self.viewport
                .y
                .saturating_add(u16::try_from(target_y).unwrap_or(u16::MAX)),
            card.width,
            u16::try_from(height).unwrap_or(u16::MAX),
        ))
    }

    pub(in crate::shell) fn render_card(
        self,
        frame: &mut Frame<'_>,
        card: CardPlacement,
        render: impl FnOnce(Rect, &mut Buffer),
    ) {
        let area = Rect::new(0, 0, card.width, card.height);
        let mut card_buffer = Buffer::empty(area);
        render(area, &mut card_buffer);

        let source_y = self.scroll.saturating_sub(card.content_y);
        let target_y = card.content_y.saturating_sub(self.scroll);
        let visible_height = usize::from(card.height)
            .saturating_sub(source_y)
            .min(usize::from(self.viewport.height).saturating_sub(target_y));
        let target_x = self.viewport.x.saturating_add(card.x);
        let target_y = self
            .viewport
            .y
            .saturating_add(u16::try_from(target_y).unwrap_or(u16::MAX));
        let source_y = u16::try_from(source_y).unwrap_or(u16::MAX);
        for y in 0..u16::try_from(visible_height).unwrap_or(u16::MAX) {
            for x in 0..card.width {
                let Some(source) = card_buffer.cell(Position::new(x, source_y.saturating_add(y)))
                else {
                    continue;
                };
                if let Some(target) = frame.buffer_mut().cell_mut(Position::new(
                    target_x.saturating_add(x),
                    target_y.saturating_add(y),
                )) {
                    *target = source.clone();
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::shell) struct CardPlacement {
    pub(in crate::shell) index: usize,
    x: u16,
    content_y: usize,
    width: u16,
    height: u16,
}

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

pub(in crate::shell) fn card_grid(
    area: Rect,
    scroll: usize,
    count: usize,
    card_height: u16,
) -> CardGrid {
    card_grid_with_gap(area, scroll, count, card_height, CARD_GAP)
}

pub(in crate::shell) fn card_grid_with_gap(
    area: Rect,
    scroll: usize,
    count: usize,
    card_height: u16,
    card_gap: u16,
) -> CardGrid {
    let (body, _) = control_content_areas(area);
    let viewport = body;
    let rows = count.div_ceil(CARD_COLUMNS);
    let content_height = rows
        .saturating_mul(usize::from(card_height + card_gap))
        .saturating_sub(usize::from(card_gap));
    let overflowing = content_height > usize::from(viewport.height);
    let viewport_width = viewport.width.saturating_sub(u16::from(overflowing));
    let viewport = Rect::new(viewport.x, viewport.y, viewport_width, viewport.height);
    let card_width = (viewport_width.saturating_sub(card_gap) / 2).max(1);
    let maximum = content_height.saturating_sub(usize::from(viewport.height));
    CardGrid {
        viewport,
        content_height,
        scroll: scroll.min(maximum),
        card_height,
        card_width,
        card_gap,
        count,
    }
}

pub(in crate::shell) fn world_card_at_position(
    area: Rect,
    scroll: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    card_grid(area, scroll, count, WORLD_CARD_HEIGHT).card_at(column, row)
}

pub(in crate::shell) fn world_card_action_at_position(
    area: Rect,
    scroll: usize,
    count: usize,
    action_width: u16,
    column: u16,
    row: u16,
) -> Option<usize> {
    let grid = card_grid(area, scroll, count, WORLD_CARD_HEIGHT);
    if !grid.viewport.contains(Position::new(column, row)) {
        return None;
    }
    grid.cards().find_map(|card| {
        let top = card.content_y.checked_sub(grid.scroll)?;
        let top = grid
            .viewport
            .y
            .saturating_add(u16::try_from(top).unwrap_or(u16::MAX));
        let right = grid
            .viewport
            .x
            .saturating_add(card.x)
            .saturating_add(card.width)
            .saturating_sub(1);
        let left = right.saturating_sub(action_width);
        (row == top && column >= left && column < right).then_some(card.index)
    })
}

pub(in crate::shell) fn pane_card_grid(area: Rect, scroll: usize, count: usize) -> CardGrid {
    card_grid_with_gap(
        area,
        scroll,
        count,
        super::super::live::CARD_HEIGHT,
        super::super::live::CARD_GAP,
    )
}

pub(in crate::shell) fn session_card_at_position(
    area: Rect,
    scroll: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    pane_card_grid(area, scroll, count).card_at(column, row)
}
