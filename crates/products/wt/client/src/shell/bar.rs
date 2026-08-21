use super::model::ShellModel;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;

pub(super) fn world_bar_label(model: &ShellModel) -> String {
    format!(
        " {} ({}/{})",
        model.active_world(),
        model.active() + 1,
        model.world_count()
    )
}

pub(super) fn world_bar_controls(model: &ShellModel, area: Rect) -> [Rect; 3] {
    let label_width = u16::try_from(Span::raw(world_bar_label(model)).width().min(24))
        .expect("world bar label width is bounded");
    let group_width = label_width.saturating_add(4).min(area.width);
    let group = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(group_width),
        Constraint::Fill(1),
    ])
    .split(Rect::new(area.x, area.y, area.width, 1))[1];
    let controls = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Length(label_width),
        Constraint::Length(2),
    ])
    .split(group);
    [controls[0], controls[1], controls[2]]
}
