use super::control::{
    command_palette_layout, control_areas, Activity, CommandPalette, ACTIVITY_BUTTON_HEIGHT,
};
use super::model::{Mode, ShellModel};
use super::world_area;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub(super) fn draw(frame: &mut Frame<'_>, screen: &vt100::Screen, model: &ShellModel) {
    if model.mode() == Mode::Control {
        draw_control(frame, model);
        return;
    }
    let world = world_area(frame.area());
    frame.render_widget(TerminalView(screen), world);
    draw_world_bar(frame, model);
    match model.mode() {
        Mode::World => {
            if !screen.hide_cursor() {
                let (row, column) = screen.cursor_position();
                frame.set_cursor_position((world.x + column, world.y + row));
            }
        }
        Mode::Switcher => {}
        Mode::Control => unreachable!("control UI returns before rendering a world"),
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

fn draw_world_bar(frame: &mut Frame<'_>, model: &ShellModel) {
    let active = model.mode() == Mode::Switcher;
    let text = if active {
        format!(
            "{} ({}/{})   ←/→ worlds   ↑ control   F5   F6 quit",
            model.active_world(),
            model.active() + 1,
            model.world_count()
        )
    } else {
        format!(
            "{} ({}/{})   F5   F6 quit",
            model.active_world(),
            model.active() + 1,
            model.world_count()
        )
    };
    let style = if active {
        Style::new()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::DarkGray).bg(Color::Black)
    };
    let areas = Layout::horizontal([Constraint::Length(8), Constraint::Min(0)]).split(Rect::new(
        frame.area().x,
        frame.area().y,
        frame.area().width,
        1,
    ));
    frame.render_widget(
        Paragraph::new("  WT").style(
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        areas[0],
    );
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(style),
        areas[1],
    );
}

fn draw_control(frame: &mut Frame<'_>, model: &ShellModel) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), area);
    let (activity_bar, content) = control_areas(area);
    draw_activity_bar(frame, activity_bar, model.control().activity());
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(content);
    let (title, placeholder) = match model.control().activity() {
        Activity::Worlds => ("Worlds", "World management"),
        Activity::Codex => ("Codex sessions", "Codex session management"),
    };
    frame.render_widget(
        Paragraph::new(placeholder)
            .alignment(Alignment::Center)
            .block(Block::new().borders(Borders::ALL).title(title)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("[ Commands (1 / F1) ] [ Activities (Tab) ] [ World (F5) ]")
            .style(Style::new().fg(Color::DarkGray)),
        rows[1],
    );
    draw_command_palette(frame, content, model.control().palette());
}

fn draw_activity_bar(frame: &mut Frame<'_>, area: Rect, active: Activity) {
    frame.render_widget(
        Block::new()
            .borders(Borders::RIGHT)
            .border_style(Style::new().fg(Color::DarkGray)),
        area,
    );
    for (index, (activity, icon)) in [(Activity::Worlds, ""), (Activity::Codex, "󰚩")]
        .into_iter()
        .enumerate()
    {
        let button = Rect::new(
            area.x,
            area.y.saturating_add(index as u16 * ACTIVITY_BUTTON_HEIGHT),
            area.width.saturating_sub(1),
            ACTIVITY_BUTTON_HEIGHT,
        );
        frame.render_widget(
            Paragraph::new(icon).alignment(Alignment::Center),
            Rect::new(button.x, button.y + 1, button.width, 1),
        );
        if activity == active {
            frame.render_widget(Paragraph::new("▌"), Rect::new(button.x, button.y + 1, 1, 1));
        }
    }
}

fn draw_command_palette(frame: &mut Frame<'_>, content: Rect, palette: &CommandPalette) {
    if !palette.is_open() {
        return;
    }
    let (area, _) = command_palette_layout(content);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::new().borders(Borders::ALL).title("Command Palette"),
        area,
    );
    let inner = area.inner(Margin::new(1, 1));
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(format!("> {}█", palette.query())), rows[0]);
    frame.render_widget(
        Paragraph::new("─".repeat(usize::from(rows[1].width)))
            .style(Style::new().fg(Color::DarkGray)),
        rows[1],
    );
    let commands = palette.matches();
    let items = if commands.is_empty() {
        vec![ListItem::new("No matching commands").style(Style::new().fg(Color::DarkGray))]
    } else {
        commands
            .iter()
            .map(|command| ListItem::new(command.label()))
            .collect()
    };
    let list = List::new(items)
        .highlight_symbol(" ")
        .highlight_style(Style::new().bg(Color::DarkGray));
    let mut state = ListState::default().with_selected(
        (!commands.is_empty()).then_some(palette.selected().min(commands.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(list, rows[2], &mut state);
    frame.render_widget(
        Paragraph::new("↑/↓ select · Enter run · Esc close")
            .style(Style::new().fg(Color::DarkGray)),
        rows[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;
    use ratatui::layout::Position;
    use ratatui::{backend::TestBackend, Terminal};

    fn parser() -> vt100::Parser {
        let mut parser = vt100::Parser::new(6, 80, 0);
        parser.process(b"world output\r\n\x1b[31mred\x1b[0m");
        parser
    }

    #[test]
    fn switcher_activates_the_world_bar() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into(), "local.two".into()]);
        model.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(5),
            crossterm::event::KeyModifiers::NONE,
        ));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, parser.screen(), &model))
            .unwrap();

        insta::assert_debug_snapshot!("shell_switcher_world_bar", terminal.backend().buffer());
        let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
        assert_eq!(brand.fg, Some(Color::Black));
        assert_eq!(brand.bg, Some(Color::Cyan));
        assert!(brand.add_modifier.contains(Modifier::BOLD));
        let style = terminal.backend().buffer().cell((8, 0)).unwrap().style();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::White));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn world_bar_is_dim_until_activated() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let model = ShellModel::new(vec!["local.one".into(), "local.two".into()]);
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, parser.screen(), &model))
            .unwrap();

        assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(3, 2));
        insta::assert_debug_snapshot!("shell_inactive_world_bar", terminal.backend().buffer());
        let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
        assert_eq!(brand.fg, Some(Color::Black));
        assert_eq!(brand.bg, Some(Color::Cyan));
        assert!(brand.add_modifier.contains(Modifier::BOLD));
        let style = terminal.backend().buffer().cell((8, 0)).unwrap().style();
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert_eq!(style.bg, Some(Color::Black));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn control_ui_has_activity_scaffolding() {
        let backend = TestBackend::new(64, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into()]);
        for code in [KeyCode::F(5), KeyCode::Up] {
            model.handle_key(crossterm::event::KeyEvent::new(
                code,
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, parser.screen(), &model))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_activities", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_opens_the_command_palette() {
        let backend = TestBackend::new(64, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into()]);
        for code in [KeyCode::F(5), KeyCode::Up, KeyCode::F(1)] {
            model.handle_key(crossterm::event::KeyEvent::new(
                code,
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, parser.screen(), &model))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_command_palette", terminal.backend().buffer());
    }
}
