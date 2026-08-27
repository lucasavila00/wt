use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use wt_control_protocol::{PaneColor, PaneFrame};

pub(super) struct TerminalView<'a>(pub(super) &'a vt100::Screen);

impl ratatui::widgets::Widget for TerminalView<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let (rows, columns) = self.0.size();
        for row in 0..rows.min(area.height) {
            for column in 0..columns.min(area.width) {
                let Some(source) = self.0.cell(row, column) else {
                    continue;
                };
                let Some(target) = buffer.cell_mut((area.x + column, area.y + row)) else {
                    continue;
                };
                let symbol = if source.is_wide_continuation() {
                    " ".to_owned()
                } else if source.has_contents() {
                    source.contents()
                } else {
                    " ".to_owned()
                };
                let (mut foreground, mut background) =
                    (color(source.fgcolor()), color(source.bgcolor()));
                if source.inverse() {
                    std::mem::swap(&mut foreground, &mut background);
                }
                let mut modifiers = Modifier::empty();
                modifiers.set(Modifier::BOLD, source.bold());
                modifiers.set(Modifier::ITALIC, source.italic());
                modifiers.set(Modifier::UNDERLINED, source.underline());
                target.set_symbol(&symbol).set_style(
                    Style::new()
                        .fg(foreground)
                        .bg(background)
                        .add_modifier(modifiers),
                );
            }
        }
    }
}

fn color(source: vt100::Color) -> Color {
    match source {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}

pub(super) struct PaneFrameView<'a>(pub(super) &'a PaneFrame);

impl ratatui::widgets::Widget for PaneFrameView<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        for row in 0..self.0.rows.min(area.height) {
            for column in 0..self.0.columns.min(area.width) {
                let Some(source) = self.0.cell(row, column) else {
                    continue;
                };
                let Some(target) = buffer.cell_mut((area.x + column, area.y + row)) else {
                    continue;
                };
                let (mut foreground, mut background) = (
                    pane_color(&source.foreground),
                    pane_color(&source.background),
                );
                if source.inverse {
                    std::mem::swap(&mut foreground, &mut background);
                }
                let mut modifiers = Modifier::empty();
                modifiers.set(Modifier::BOLD, source.bold);
                modifiers.set(Modifier::ITALIC, source.italic);
                modifiers.set(Modifier::UNDERLINED, source.underlined);
                target.set_symbol(&source.text).set_style(
                    Style::new()
                        .fg(foreground)
                        .bg(background)
                        .add_modifier(modifiers),
                );
            }
        }
    }
}

fn pane_color(source: &PaneColor) -> Color {
    match source {
        PaneColor::Default => Color::Reset,
        PaneColor::Indexed { index } => Color::Indexed(*index),
        PaneColor::Rgb { red, green, blue } => Color::Rgb(*red, *green, *blue),
    }
}
