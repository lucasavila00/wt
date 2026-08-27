#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Activity {
    Worlds,
    Live,
}

impl Activity {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Worlds => Self::Live,
            Self::Live => Self::Worlds,
        }
    }
}

pub(super) fn at_position(area: ratatui::layout::Rect, column: u16, row: u16) -> Option<Activity> {
    let (bar, _) = super::control::control_areas(area);
    if column < bar.x
        || column >= bar.right().saturating_sub(1)
        || row < bar.y
        || row >= bar.bottom()
    {
        return None;
    }
    match row.saturating_sub(bar.y) / super::control::ACTIVITY_BUTTON_HEIGHT {
        0 => Some(Activity::Live),
        1 => Some(Activity::Worlds),
        _ => None,
    }
}

pub(super) fn draw(frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect, active: Activity) {
    use ratatui::layout::{Alignment, Rect};
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    frame.render_widget(
        Block::new()
            .borders(Borders::RIGHT)
            .border_style(Style::new()),
        area,
    );
    for (index, (activity, icon)) in [(Activity::Live, "󰆍"), (Activity::Worlds, "")]
        .into_iter()
        .enumerate()
    {
        let button = Rect::new(
            area.x,
            area.y
                .saturating_add(index as u16 * super::control::ACTIVITY_BUTTON_HEIGHT),
            area.width.saturating_sub(1),
            super::control::ACTIVITY_BUTTON_HEIGHT,
        );
        let active_style = (activity == active).then(|| Style::new().fg(Color::Blue));
        frame.render_widget(
            Paragraph::new(icon)
                .alignment(Alignment::Center)
                .style(active_style.unwrap_or_default()),
            Rect::new(button.x, button.y + 1, button.width, 1),
        );
        if activity == active {
            frame.render_widget(
                Paragraph::new("▌").style(active_style.unwrap()),
                Rect::new(button.x, button.y + 1, 1, 1),
            );
        }
    }
}
