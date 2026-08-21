use super::model::{Mode, ShellModel};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub(super) fn draw(frame: &mut Frame<'_>, screen: &vt100::Screen, model: &ShellModel) {
    frame.render_widget(TerminalView(screen), frame.area());
    match model.mode() {
        Mode::World => {
            if !screen.hide_cursor() {
                let (row, column) = screen.cursor_position();
                frame.set_cursor_position((column, row));
            }
        }
        Mode::Switcher => draw_switcher(frame),
        Mode::Control => draw_control(frame),
    }
}

struct TerminalView<'a>(&'a vt100::Screen);

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

fn draw_switcher(frame: &mut Frame<'_>) {
    draw_overlay(frame, 42, 3, "←/→ worlds   ↑ control   F5 close".to_owned());
}

fn draw_control(frame: &mut Frame<'_>) {
    draw_overlay(frame, 32, 5, "CONTORL UI\n\nF5 close".to_owned());
}

fn draw_overlay(frame: &mut Frame<'_>, width: u16, height: u16, text: String) {
    let outer = frame.area();
    let area = Rect::new(
        outer.x + outer.width.saturating_sub(width) / 2,
        outer.y,
        width.min(outer.width),
        height.min(outer.height),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::new().fg(Color::White).bg(Color::Black))
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .style(Style::new().fg(Color::White).bg(Color::Black)),
            ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;
    use ratatui::{backend::TestBackend, Terminal};

    fn parser() -> vt100::Parser {
        let mut parser = vt100::Parser::new(6, 80, 0);
        parser.process(b"world output\r\n\x1b[31mred\x1b[0m");
        parser
    }

    #[test]
    fn switcher_is_drawn_over_the_live_world() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into(), "local.two".into()]);
        model.handle_key(KeyCode::F(5));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, parser.screen(), &model))
            .unwrap();

        insta::assert_debug_snapshot!("shell_switcher_over_world", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_is_only_the_placeholder() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into()]);
        model.handle_key(KeyCode::F(5));
        model.handle_key(KeyCode::Up);
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, parser.screen(), &model))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_placeholder", terminal.backend().buffer());
    }
}
