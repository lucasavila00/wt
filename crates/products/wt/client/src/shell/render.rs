use super::control::{
    codex_card_rects, command_palette_layout, control_areas, control_content_areas, Activity,
    CodexCard, CodexCardKind, CommandPalette, ControlState, ACTIVITY_BUTTON_HEIGHT,
};
use super::model::{Mode, ShellModel};
use super::world_area;
use crate::create::Flow;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

pub(super) fn draw(
    frame: &mut Frame<'_>,
    screen: Option<&vt100::Screen>,
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
    let (body, footer) = control_content_areas(area);
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
            body,
        ),
        Activity::Codex => draw_codex(frame, body, model.control()),
    }
    let hint = match (model.control().activity(), model.has_worlds()) {
        (Activity::Worlds, true) => "[ Commands (1 / F1) ] [ Activities (Tab) ] [ World (F5) ]",
        (Activity::Worlds, false) => "[ Commands (1 / F1) ] [ Activities (Tab) ] [ Close (F6) ]",
        (Activity::Codex, true) => {
            "[ ↑/↓ or wheel: select ] [ Enter/click: open ] [ Tab: activity ] [ F5: world ]"
        }
        (Activity::Codex, false) => {
            "[ ↑/↓ or wheel: select ] [ Enter/click: open ] [ Tab: activity ] [ Close (F6) ]"
        }
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::new().fg(Color::DarkGray)),
        footer,
    );
    draw_command_palette(frame, content, model.control().palette());
}

fn refresh_title(label: &str, updated_at: Option<&str>) -> String {
    updated_at.map_or_else(
        || format!("{label} · Updating…"),
        |updated_at| format!("{label} · Last updated {updated_at}"),
    )
}

fn draw_codex(frame: &mut Frame<'_>, area: Rect, state: &ControlState) {
    let block = Block::new()
        .borders(Borders::ALL)
        .title(refresh_title("Codex sessions", state.codex_updated_at()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if state.codex().is_empty() {
        let message = if state.codex_updated_at().is_some() {
            "No Codex sessions\nStart Codex in a world to see its session here"
        } else {
            "Loading Codex sessions…"
        };
        frame.render_widget(Paragraph::new(message).alignment(Alignment::Center), inner);
        return;
    }
    for (index, rect) in codex_card_rects(frame.area(), state.codex_offset(), state.codex().len()) {
        let card = &state.codex()[index];
        draw_codex_card(
            frame,
            rect,
            card,
            state,
            state.selected() == Some(&card.identity),
        );
    }
}

fn draw_codex_card(
    frame: &mut Frame<'_>,
    area: Rect,
    card: &CodexCard,
    state: &ControlState,
    selected: bool,
) {
    let (title, title_color) = card_title(card);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(if selected {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(title_color).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(error) = state.open_error(&card.identity) {
        frame.render_widget(
            Paragraph::new(Line::styled(error, Style::new().fg(Color::Red)))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let mut lines = card_lines(card);
    let footer = if state.opening() == Some(&card.identity) {
        Span::styled("OPENING…", Style::new().fg(Color::Yellow))
    } else if let Some(reason) = card.disabled_reason() {
        Span::styled(
            format!("Unavailable: {reason}"),
            Style::new().fg(Color::DarkGray),
        )
    } else {
        Span::styled("Enter or click to open", Style::new().fg(Color::DarkGray))
    };
    lines.push(Line::from(footer));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn card_title(card: &CodexCard) -> (String, Color) {
    let suffix = card
        .timestamp
        .map(relative_age)
        .map_or_else(String::new, |age| format!(" · {age}"));
    match &card.kind {
        CodexCardKind::Observation { state, .. } => {
            let (icon, label, color) = match state {
                wt_control_protocol::CodexSessionState::NeedsAttention => {
                    ("󰚩", "NEEDS ATTENTION", Color::Yellow)
                }
                wt_control_protocol::CodexSessionState::Working => ("󰔟", "WORKING", Color::Green),
                wt_control_protocol::CodexSessionState::Unknown => ("󰋗", "UNKNOWN", Color::Gray),
                wt_control_protocol::CodexSessionState::Inactive => {
                    ("󰅖", "INACTIVE", Color::DarkGray)
                }
            };
            (format!("{icon} {label}{suffix}"), color)
        }
        CodexCardKind::RolloutOnly => (format!("󰈙 ROLLOUT ONLY{suffix}"), Color::DarkGray),
        CodexCardKind::ContextError { .. } => ("󰅚 CONTEXT ERROR".into(), Color::Red),
    }
}

fn card_lines(card: &CodexCard) -> Vec<Line<'static>> {
    let short_session = card
        .session_id
        .map(|session| session.to_string()[..8].to_owned());
    match &card.kind {
        CodexCardKind::Observation {
            world_name,
            cwd,
            target,
            ..
        } => vec![
            Line::from(format!(
                "{}.{} · {}:{} · session {}",
                card.context,
                world_name,
                target.tmux_session,
                target.pane_id,
                short_session.expect("observation card has session ID")
            )),
            Line::from(cwd.clone()),
        ],
        CodexCardKind::RolloutOnly => vec![
            Line::from(format!(
                "{} · session {}",
                card.context,
                short_session.expect("rollout card has session ID")
            )),
            Line::from("Durable rollout; no live WT pane was reported"),
        ],
        CodexCardKind::ContextError { message } => vec![
            Line::from(format!("Context {}", card.context)),
            Line::styled(message.clone(), Style::new().fg(Color::Red)),
        ],
    }
}

fn relative_age(timestamp: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(timestamp);
    let (future, milliseconds) = if timestamp > now {
        (true, timestamp - now)
    } else {
        (false, now - timestamp)
    };
    let seconds = milliseconds / 1000;
    let value = if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 60 * 60 {
        format!("{}m", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h", seconds / (60 * 60))
    } else {
        format!("{}d", seconds / (24 * 60 * 60))
    };
    if future {
        format!("in {value}")
    } else {
        format!("{value} ago")
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
    use crate::shell::codex::ShellWorld;
    use crate::shell::control::{CodexCardIdentity, CodexCardKind};
    use crossterm::event::KeyCode;
    use ratatui::layout::Position;
    use ratatui::{backend::TestBackend, Terminal};
    use uuid::Uuid;
    use wt_control_protocol::{ByobuTarget, CodexSessionState};

    fn parser() -> vt100::Parser {
        let mut parser = vt100::Parser::new(6, 80, 0);
        parser.process(b"world output\r\n\x1b[31mred\x1b[0m");
        parser
    }

    fn model(names: &[&str]) -> ShellModel {
        ShellModel::new(
            names
                .iter()
                .enumerate()
                .map(|(index, name)| ShellWorld::test(name, index as u128 + 1))
                .collect(),
        )
    }

    fn press(model: &mut ShellModel, code: KeyCode, area: Rect) {
        model.handle_key(
            crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
            area,
        );
    }

    #[test]
    fn switcher_activates_the_world_bar() {
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = model(&["local.one", "local.two"]);
        press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
        press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), &model, None, None))
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
        let mut model = model(&["local.one", "local.two"]);
        press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), &model, None, None))
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
        model.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::F(5), crossterm::event::KeyModifiers::SHIFT),
            Rect::new(0, 0, 80, 6),
        );
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_disabled_f5_override", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_has_activity_scaffolding() {
        let backend = TestBackend::new(64, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let model = model(&["local.one"]);
        assert_eq!(model.mode(), Mode::Control);
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_activities", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_opens_the_command_palette() {
        let backend = TestBackend::new(64, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = model(&["local.one"]);
        press(&mut model, KeyCode::F(1), Rect::new(0, 0, 64, 16));
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_control_command_palette", terminal.backend().buffer());
    }

    #[test]
    fn control_ui_shows_codex_session_cards() {
        let backend = TestBackend::new(100, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = model(&["ars.dev"]);
        let session_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let world_id = Uuid::parse_str("223e4567-e89b-12d3-a456-426614174000").unwrap();
        let target = ByobuTarget {
            tmux_session: "wt-app".into(),
            pane_id: "%1".into(),
        };
        model.set_codex(
            vec![
                CodexCard {
                    identity: CodexCardIdentity::Observation {
                        context: "ars".into(),
                        session_id,
                        world_id,
                        tmux_session: target.tmux_session.clone(),
                        pane_id: target.pane_id.clone(),
                    },
                    context: "ars".into(),
                    session_id: Some(session_id),
                    timestamp: Some(now_ms()),
                    kind: CodexCardKind::Observation {
                        world_id,
                        world_name: "dev".into(),
                        cwd: "/workspace/wt".into(),
                        state: CodexSessionState::NeedsAttention,
                        target,
                    },
                },
                CodexCard::rollout_only(
                    "ars",
                    Uuid::parse_str("323e4567-e89b-12d3-a456-426614174000").unwrap(),
                    now_ms(),
                ),
                CodexCard::context_error("lab", "context lab: SSH failed".into()),
            ],
            "2026-08-21T20:00:00Z".into(),
            Rect::new(0, 0, 100, 22),
        );
        let parser = parser();

        terminal
            .draw(|frame| draw(frame, Some(parser.screen()), &model, None, None))
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
            .draw(|frame| draw(frame, None, &model, None, None))
            .unwrap();

        insta::assert_debug_snapshot!("shell_empty_control", terminal.backend().buffer());
    }

    fn now_ms() -> i64 {
        i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap()
    }
}
