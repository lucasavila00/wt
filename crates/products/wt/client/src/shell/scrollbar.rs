use ratatui::{
    layout::{Margin, Position, Rect},
    style::Style,
    symbols::{block, line},
    Frame,
};

use super::control::{
    card_grid_visible, control_content_areas, CODEX_CARD_HEIGHT, WORLD_CARD_HEIGHT,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScrollbarGeometry {
    thumb_start: usize,
    thumb_length: usize,
}

pub(super) fn area(area: Rect) -> Rect {
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    Rect::new(
        viewport.right().saturating_sub(1),
        viewport.y,
        1.min(viewport.width),
        viewport.height,
    )
}

pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    content_length: usize,
    viewport_length: usize,
    position: usize,
    style: Style,
) {
    if area.is_empty() || viewport_length >= content_length {
        return;
    }
    let track_length = usize::from(area.height);
    let geometry = scrollbar_geometry(track_length, content_length, viewport_length, position);
    for offset in 0..track_length {
        let symbol = if offset >= geometry.thumb_start
            && offset < geometry.thumb_start.saturating_add(geometry.thumb_length)
        {
            block::FULL
        } else {
            line::DOUBLE_VERTICAL
        };
        let offset = u16::try_from(offset).unwrap_or(u16::MAX);
        if let Some(cell) = frame
            .buffer_mut()
            .cell_mut(Position::new(area.x, area.y.saturating_add(offset)))
        {
            cell.set_symbol(symbol).set_style(style);
        }
    }
}

pub(super) fn render_world_cards(
    frame: &mut Frame<'_>,
    count: usize,
    selected: usize,
    style: Style,
) {
    let viewport = card_grid_visible(frame.area(), WORLD_CARD_HEIGHT).max(1);
    render(
        frame,
        area(frame.area()),
        count,
        viewport,
        selected / viewport * viewport,
        style,
    );
}

pub(super) fn render_codex_cards(frame: &mut Frame<'_>, count: usize, offset: usize, style: Style) {
    render(
        frame,
        area(frame.area()),
        count,
        card_grid_visible(frame.area(), CODEX_CARD_HEIGHT).max(1),
        offset,
        style,
    );
}

fn scrollbar_geometry(
    track_length: usize,
    content_length: usize,
    viewport_length: usize,
    position: usize,
) -> ScrollbarGeometry {
    if track_length == 0 {
        return ScrollbarGeometry {
            thumb_start: 0,
            thumb_length: 0,
        };
    }

    let thumb_length = if content_length == 0 || viewport_length >= content_length {
        track_length
    } else {
        rounded_ratio(viewport_length, track_length, content_length).clamp(1, track_length)
    };
    let maximum = content_length.saturating_sub(viewport_length);
    let available_travel = track_length.saturating_sub(thumb_length);
    let thumb_start = if maximum == 0 {
        0
    } else {
        rounded_ratio(position.min(maximum), available_travel, maximum).min(available_travel)
    };

    ScrollbarGeometry {
        thumb_start,
        thumb_length,
    }
}

fn rounded_ratio(value: usize, scale: usize, divisor: usize) -> usize {
    let value = u128::try_from(value).unwrap_or(u128::MAX);
    let scale = u128::try_from(scale).unwrap_or(u128::MAX);
    let divisor = u128::try_from(divisor).unwrap_or(u128::MAX);
    let rounded = value.saturating_mul(scale).saturating_add(divisor / 2) / divisor;
    usize::try_from(rounded).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_length_is_stable_at_every_legal_position() {
        for track_length in 1..=24 {
            for content_length in 1..=48 {
                for viewport_length in 1..=content_length {
                    let maximum = content_length - viewport_length;
                    let expected =
                        scrollbar_geometry(track_length, content_length, viewport_length, 0)
                            .thumb_length;
                    for position in 0..=maximum {
                        assert_eq!(
                            scrollbar_geometry(
                                track_length,
                                content_length,
                                viewport_length,
                                position,
                            )
                            .thumb_length,
                            expected,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn thumb_reaches_both_track_endpoints() {
        let first = scrollbar_geometry(17, 100, 20, 0);
        let last = scrollbar_geometry(17, 100, 20, 80);

        assert_eq!(first.thumb_start, 0);
        assert_eq!(last.thumb_start + last.thumb_length, 17);
    }

    #[test]
    fn geometry_handles_empty_oversized_and_clamped_inputs() {
        assert_eq!(
            scrollbar_geometry(3, 10_000, 1, 0),
            ScrollbarGeometry {
                thumb_start: 0,
                thumb_length: 1,
            }
        );
        assert_eq!(
            scrollbar_geometry(0, 100, 20, 80),
            ScrollbarGeometry {
                thumb_start: 0,
                thumb_length: 0,
            }
        );
        assert_eq!(
            scrollbar_geometry(7, 5, 10, usize::MAX),
            ScrollbarGeometry {
                thumb_start: 0,
                thumb_length: 7,
            }
        );
        assert_eq!(
            scrollbar_geometry(10, 100, 20, usize::MAX),
            scrollbar_geometry(10, 100, 20, 80),
        );
    }
}
