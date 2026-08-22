use ratatui::layout::Rect;

const WIDTH: u16 = 52;
const HEIGHT: u16 = 5;

pub(super) fn area(outer: Rect) -> Rect {
    let width = WIDTH.min(outer.width.saturating_sub(2));
    let height = HEIGHT.min(outer.height.saturating_sub(2));
    Rect::new(
        outer.right().saturating_sub(width).saturating_sub(1),
        outer.bottom().saturating_sub(height).saturating_sub(1),
        width,
        height,
    )
}

pub(super) fn actions(outer: Rect) -> (Rect, Rect) {
    let toast = area(outer);
    let retry = Rect::new(
        toast.x.saturating_add(1),
        toast.bottom().saturating_sub(2),
        toast.width.saturating_sub(2),
        1,
    );
    let dismiss = Rect::new(toast.right().saturating_sub(2), toast.y, 1, 1);
    (retry, dismiss)
}
