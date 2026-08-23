use super::model::ShellModel;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;

pub(super) const BRAND_LABEL: &str = "  WT ";
pub(super) const CONTROL_LABEL: &str = " F5: ctrl ";
pub(super) const CLOSE_LABEL: &str = " F6: close ";

pub(super) fn world_bar_brand(area: Rect) -> Rect {
    let width =
        u16::try_from(Span::raw(BRAND_LABEL).width()).expect("brand label width is bounded");
    Rect::new(area.x, area.y, width.min(area.width), 1)
}

pub(super) fn world_bar_label(model: &ShellModel) -> String {
    format!(
        " {} ({}/{})",
        model.active_world(),
        model.active() + 1,
        model.world_count()
    )
}

pub(super) fn world_bar_world(model: &ShellModel, area: Rect) -> Rect {
    let label_width = u16::try_from(Span::raw(world_bar_label(model)).width().min(24))
        .expect("world bar label width is bounded");
    Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(label_width),
        Constraint::Fill(1),
    ])
    .split(Rect::new(area.x, area.y, area.width, 1))[1]
}

pub(super) fn world_bar_control(area: Rect) -> Rect {
    let control_width =
        u16::try_from(Span::raw(CONTROL_LABEL).width()).expect("control label width is bounded");
    let close_width =
        u16::try_from(Span::raw(CLOSE_LABEL).width()).expect("close label width is bounded");
    let controls = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(control_width),
        Constraint::Length(close_width),
    ])
    .split(area);
    controls[1]
}
