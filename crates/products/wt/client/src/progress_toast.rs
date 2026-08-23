use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::time::Instant;

pub(crate) struct ProgressToast {
    visible: bool,
    started: Instant,
}

impl ProgressToast {
    pub(crate) fn new() -> Self {
        Self {
            visible: true,
            started: Instant::now(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.visible = true;
        self.started = Instant::now();
    }

    pub(crate) fn render(
        &self,
        frame: &mut Frame<'_>,
        outer: Rect,
        title: &str,
        subject: &str,
        status: &str,
    ) {
        if !self.visible {
            return;
        }
        let elapsed = self.started.elapsed();
        const GRADIENT: [u8; 12] = [24, 25, 31, 37, 43, 42, 36, 30, 24, 60, 54, 53];
        let animation_tick = elapsed.as_millis() as usize / 25;
        let spinner = ["", "", "", ""][(animation_tick / 2) % 4];
        let Some(area) = area(outer) else {
            return;
        };
        frame.render_widget(Clear, area);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(format!(" {title} "))
            .title(Line::from("×").alignment(Alignment::Right));
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(format!("{spinner} {subject}\n{status}"))
                .wrap(Wrap { trim: false })
                .style(Style::new().fg(Color::Indexed(
                    GRADIENT[(animation_tick / 4) % GRADIENT.len()],
                ))),
            area.inner(Margin::new(1, 1)),
        );
    }

    pub(crate) fn handle_mouse(&mut self, event: &Event, outer: Rect) -> bool {
        if !self.visible {
            return false;
        }
        let Event::Mouse(mouse) = event else {
            return false;
        };
        let Some(area) = area(outer) else {
            return false;
        };
        let position = (mouse.column, mouse.row).into();
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && dismiss_area(area).contains(position)
        {
            self.visible = false;
            return true;
        }
        area.contains(position)
    }
}

pub(crate) fn area(outer: Rect) -> Option<Rect> {
    let width = 44.min(outer.width.saturating_sub(2));
    if width < 24 || outer.height < 6 {
        return None;
    }
    Some(Rect::new(
        outer.right().saturating_sub(1).saturating_sub(width),
        outer.y.saturating_add(1),
        width,
        5,
    ))
}

fn dismiss_area(area: Rect) -> Rect {
    Rect::new(area.right().saturating_sub(2), area.y, 1, 1)
}
