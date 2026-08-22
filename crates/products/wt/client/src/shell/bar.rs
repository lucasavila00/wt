use super::model::ShellModel;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;

pub(super) const PREVIOUS_LABEL: &str = "← PREV ";
pub(super) const NEXT_LABEL: &str = " NEXT →";

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
    let previous_width = u16::try_from(Span::raw(PREVIOUS_LABEL).width())
        .expect("previous world label width is bounded");
    let next_width =
        u16::try_from(Span::raw(NEXT_LABEL).width()).expect("next world label width is bounded");
    let group_width = label_width
        .saturating_add(previous_width)
        .saturating_add(next_width)
        .min(area.width);
    let group = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(group_width),
        Constraint::Fill(1),
    ])
    .split(Rect::new(area.x, area.y, area.width, 1))[1];
    let controls = Layout::horizontal([
        Constraint::Length(previous_width),
        Constraint::Length(label_width),
        Constraint::Length(next_width),
    ])
    .split(group);
    [controls[0], controls[1], controls[2]]
}
