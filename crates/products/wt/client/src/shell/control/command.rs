use super::ControlCommand;
use ratatui::layout::{Constraint, Layout, Margin, Rect};

impl ControlCommand {
    pub(in crate::shell) fn label(self) -> &'static str {
        match self {
            Self::NewWorld => "World: New",
            Self::DeleteWorld => "World: Delete...",
        }
    }
}

pub(in crate::shell) fn command_palette_layout(content: Rect) -> (Rect, Rect) {
    let width = (content.width.saturating_mul(70) / 100)
        .clamp(30.min(content.width), 70.min(content.width));
    let height = 9.min(content.height);
    let area = Rect::new(
        content.x + content.width.saturating_sub(width) / 2,
        content.y + content.height.saturating_mul(20) / 100,
        width,
        height,
    );
    let inner = area.inner(Margin::new(1, 1));
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    (area, rows[2])
}
