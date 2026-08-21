use super::control::{
    command_palette_layout, control_areas, Activity, CodexContextSnapshot, CommandPalette,
    ACTIVITY_BUTTON_HEIGHT,
};
use super::model::{Mode, ShellModel};
use super::world_area;
use crate::create::Flow;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table};
use ratatui::Frame;

pub(super) fn draw(
    frame: &mut Frame<'_>,
    screen: Option<&vt100::Screen>,
    closed_message: Option<&str>,
    model: &ShellModel,
    creation: Option<&Flow>,
    creation_error: Option<&str>,
) {
    if model.mode() == Mode::Control {
        if let Some(creation) = creation {
            creation.render(frame, frame.area());
            return;
        }
        draw_control(frame, model);
        if let Some(error) = creation_error {
            draw_creation_error(frame, error);
        }
        return;
    }
    let screen = screen.expect("world mode requires a world screen");
    let world = world_area(frame.area());
    frame.render_widget(TerminalView(screen), world);
    draw_world_bar(frame, model);
    if let Some(message) = closed_message {
        draw_closed_session_bar(frame, message);
    }
    match model.mode() {
        Mode::World if closed_message.is_none() => {
            if !screen.hide_cursor() {
                let (row, column) = screen.cursor_position();
                frame.set_cursor_position((world.x + column, world.y + row));
            }
        }
        Mode::World | Mode::Switcher => {}
        Mode::Control => unreachable!("control UI returns before rendering a world"),
    }
}

fn draw_closed_session_bar(frame: &mut Frame<'_>, message: &str) {
    let area = frame.area();
    frame.render_widget(
        Paragraph::new(format!(" {message} · Space: reconnect "))
            .alignment(Alignment::Center)
            .style(
                Style::new()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
        Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
    );
}

fn draw_creation_error(frame: &mut Frame<'_>, error: &str) {
    let outer = frame.area();
    let width = 70.min(outer.width);
    let height = 12.min(outer.height);
    let area = Rect::new(
        outer.x + outer.width.saturating_sub(width) / 2,
        outer.y + outer.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(error)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .title("World creation unavailable")
                    .title_bottom(" Enter/Esc close "),
            ),
        area,
    );
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
    let disabled = model.f5_disabled();
    let active = model.mode() == Mode::Switcher;
    let left_hint = if disabled {
        " F5 disabled"
    } else if active {
        " F5: disable navbar"
    } else {
        " F5: enable navbar"
    };
    let right_hint = if disabled {
        "Shift+F5: enable F6: close "
    } else if active {
        "←/→ world ↑ ctrl F6: close "
    } else {
        "F6: close "
    };
    let style = if disabled {
        Style::new()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::new()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::DarkGray).bg(Color::Black)
    };
    let areas = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(24),
        Constraint::Fill(1),
    ])
    .split(Rect::new(
        frame.area().x,
        frame.area().y,
        frame.area().width,
        1,
    ));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  WT ",
                if disabled {
                    style
                } else {
                    Style::new()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                },
            ),
            Span::raw(left_hint),
        ]))
        .style(style),
        areas[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            " {} ({}/{})",
            model.active_world(),
            model.active() + 1,
            model.world_count()
        ))
        .alignment(Alignment::Center)
        .style(style),
        areas[1],
    );
    frame.render_widget(
        Paragraph::new(right_hint)
            .alignment(Alignment::Right)
            .style(style),
        areas[2],
    );
}

fn draw_control(frame: &mut Frame<'_>, model: &ShellModel) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), area);
    let (activity_bar, content) = control_areas(area);
    draw_activity_bar(frame, activity_bar, model.control().activity());
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(content);
    match model.control().activity() {
        Activity::Worlds => frame.render_widget(
            Paragraph::new(if model.has_worlds() {
                "World management"
            } else {
                "No worlds with SSH access\nCreate a world to get started"
            })
            .alignment(Alignment::Center)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .title(refresh_title("Worlds", model.control().worlds_updated_at())),
            ),
            rows[0],
        ),
        Activity::Codex => draw_codex(
            frame,
            rows[0],
            model.control().codex(),
            model.control().codex_updated_at(),
        ),
    }
    frame.render_widget(
        Paragraph::new(if model.has_worlds() {
            "[ Commands (1 / F1) ] [ Activities (Tab) ] [ World (F5) ]"
        } else {
            "[ Commands (1 / F1) ] [ Activities (Tab) ] [ Close (F6) ]"
        })
        .style(Style::new().fg(Color::DarkGray)),
        rows[1],
    );
    draw_command_palette(frame, content, model.control().palette());
}

fn refresh_title(label: &str, updated_at: Option<&str>) -> String {
    updated_at.map_or_else(
        || format!("{label} · Updating…"),
        |updated_at| format!("{label} · Last updated {updated_at}"),
    )
}

fn draw_codex(
    frame: &mut Frame<'_>,
    area: Rect,
    contexts: &[CodexContextSnapshot],
    updated_at: Option<&str>,
) {
    let block = Block::new()
        .borders(Borders::ALL)
        .title(refresh_title("Codex sessions", updated_at));
    let rows = contexts.iter().flat_map(codex_rows).collect::<Vec<_>>();
    if rows.is_empty() {
        let message = if updated_at.is_some() {
            "No Codex sessions\nStart Codex in a world to see its session here"
        } else {
            "Loading Codex sessions…"
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .block(block),
            area,
        );
        return;
    }
    let table = Table::new(
        rows,
        [
            Constraint::Length(11),
            Constraint::Length(20),
            Constraint::Length(11),
            Constraint::Length(10),
            Constraint::Min(8),
        ],
    )
    .column_spacing(1)
    .header(
        Row::new(["STATE", "WORLD", "PANE", "SESSION", "CWD"])
            .style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .block(block);
    frame.render_widget(table, area);
}

fn codex_rows(context: &CodexContextSnapshot) -> Vec<Row<'static>> {
    match context {
        CodexContextSnapshot::Failure { context, message } => vec![Row::new([
            "error".to_owned(),
            context.clone(),
            "-".to_owned(),
            "-".to_owned(),
            message.lines().next().unwrap_or_default().to_owned(),
        ])
        .style(Style::new().fg(Color::Red))],
        CodexContextSnapshot::Sessions { context, sessions } => sessions
            .iter()
            .flat_map(|session| {
                let session_id = session.session_id.to_string()[..8].to_owned();
                if session.observations.is_empty() {
                    return vec![Row::new([
                        "catalog".to_owned(),
                        context.clone(),
                        "-".to_owned(),
                        session_id,
                        "-".to_owned(),
                    ])];
                }
                session
                    .observations
                    .iter()
                    .map(|observation| {
                        Row::new([
                            codex_state(observation.state).to_owned(),
                            format!("{context}.{}", observation.world_name),
                            format!(
                                "{}:{}",
                                observation.target.tmux_session, observation.target.pane_id
                            ),
                            session_id.clone(),
                            observation.cwd.clone(),
                        ])
                    })
                    .collect()
            })
            .collect(),
    }
}

fn codex_state(state: wt_control_protocol::CodexSessionState) -> &'static str {
    match state {
        wt_control_protocol::CodexSessionState::Unknown => "unknown",
        wt_control_protocol::CodexSessionState::Working => "working",
        wt_control_protocol::CodexSessionState::NeedsAttention => "attention",
        wt_control_protocol::CodexSessionState::Inactive => "inactive",
    }
}

fn draw_activity_bar(frame: &mut Frame<'_>, area: Rect, active: Activity) {
    frame.render_widget(
        Block::new()
            .borders(Borders::RIGHT)
            .border_style(Style::new().fg(Color::DarkGray)),
        area,
    );
    for (index, (activity, icon)) in [(Activity::Codex, "󰚩"), (Activity::Worlds, "")]
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
    use crate::shell::control::CodexContextSnapshot;
    use crossterm::event::KeyCode;
    use ratatui::layout::Position;
    use ratatui::{backend::TestBackend, Terminal};
    use uuid::Uuid;
    use wt_control_protocol::{
        ByobuTarget, CodexSession, CodexSessionObservation, CodexSessionState, InstanceName,
    };

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
        model.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(5),
            crossterm::event::KeyModifiers::NONE,
        ));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_switcher_world_bar", terminal.backend().buffer());
        let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
        assert_eq!(brand.fg, Some(Color::Black));
        assert_eq!(brand.bg, Some(Color::Cyan));
        assert!(brand.add_modifier.contains(Modifier::BOLD));
        let style = terminal.backend().buffer().cell((6, 0)).unwrap().style();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::White));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn world_bar_is_dim_until_activated() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into(), "local.two".into()]);
        model.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(5),
            crossterm::event::KeyModifiers::NONE,
        ));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None))
            .unwrap();

        assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(3, 2));
        insta::assert_debug_snapshot!("shell_inactive_world_bar", terminal.backend().buffer());
        let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
        assert_eq!(brand.fg, Some(Color::Black));
        assert_eq!(brand.bg, Some(Color::Cyan));
        assert!(brand.add_modifier.contains(Modifier::BOLD));
        let style = terminal.backend().buffer().cell((6, 0)).unwrap().style();
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert_eq!(style.bg, Some(Color::Black));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn disabled_f5_override_has_a_red_top_bar() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into()]);
        model.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(5),
            crossterm::event::KeyModifiers::SHIFT,
        ));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_disabled_f5_override", terminal.backend().buffer());
    }

    #[test]
    fn closed_session_has_a_red_reconnect_bar() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into()]);
        model.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(5),
            crossterm::event::KeyModifiers::NONE,
        ));
        let parser = parser();

        terminal
            .draw(|frame| {
                draw(
                    frame,
                    Some(parser.screen()),
                    Some("SSH session ended: Exited with code 255"),
                    &model,
                    None,
                    None,
                )
            })
            .unwrap();

        insta::assert_debug_snapshot!("shell_closed_session", terminal.backend().buffer());
        let status = terminal.backend().buffer().cell((0, 5)).unwrap().style();
        assert_eq!(status.fg, Some(Color::White));
        assert_eq!(status.bg, Some(Color::Red));
        assert!(status.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn control_ui_has_activity_scaffolding() {
        let backend = TestBackend::new(64, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let model = ShellModel::new(vec!["local.one".into()]);
        assert_eq!(model.mode(), Mode::Control);
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_activities", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_opens_the_command_palette() {
        let backend = TestBackend::new(64, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["local.one".into()]);
        model.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::F(1),
            crossterm::event::KeyModifiers::NONE,
        ));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_command_palette", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_shows_read_only_codex_sessions() {
        let backend = TestBackend::new(100, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = ShellModel::new(vec!["ars.dev".into()]);
        model.set_codex(
            vec![
                CodexContextSnapshot::Sessions {
                    context: "ars".into(),
                    sessions: vec![
                        CodexSession {
                            session_id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000")
                                .unwrap(),
                            rollout_updated_at_unix_ms: Some(10),
                            observations: vec![CodexSessionObservation {
                                world_id: Uuid::parse_str("223e4567-e89b-12d3-a456-426614174000")
                                    .unwrap(),
                                world_name: InstanceName::parse("dev").unwrap(),
                                cwd: "/workspace/wt".into(),
                                state: CodexSessionState::NeedsAttention,
                                target: ByobuTarget {
                                    tmux_session: "wt-app".into(),
                                    pane_id: "%1".into(),
                                },
                                received_at_unix_ms: 20,
                            }],
                        },
                        CodexSession {
                            session_id: Uuid::parse_str("323e4567-e89b-12d3-a456-426614174000")
                                .unwrap(),
                            rollout_updated_at_unix_ms: Some(30),
                            observations: vec![],
                        },
                    ],
                },
                CodexContextSnapshot::Failure {
                    context: "lab".into(),
                    message: "context lab could not be queried: SSH failed".into(),
                },
            ],
            "2026-08-21T20:00:00Z".into(),
        );
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_codex_sessions", terminal.backend().buffer());
    }

    #[test]
    fn refresh_titles_distinguish_waiting_from_applied_snapshots() {
        assert_eq!(
            refresh_title("Codex sessions", None),
            "Codex sessions · Updating…"
        );
        assert_eq!(
            refresh_title("Codex sessions", Some("2026-08-21T20:00:00Z")),
            "Codex sessions · Last updated 2026-08-21T20:00:00Z"
        );
    }

    #[test]
    fn empty_shell_renders_the_control_ui() {
        let backend = TestBackend::new(64, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let model = ShellModel::new(Vec::new());

        terminal
            .draw(|frame| draw(frame, None, None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_empty_control", terminal.backend().buffer());
    }
}
