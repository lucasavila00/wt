use super::control::{
    codex_card_rects, command_palette_layout, control_areas, control_content_areas,
    world_card_rects, Activity, CodexCard, CodexCardKind, CommandPalette, ControlState,
    ACTIVITY_BUTTON_HEIGHT,
};
use super::delete;
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
    closed_message: Option<&str>,
    model: &ShellModel,
    creation: Option<&Flow>,
    creation_error: Option<&str>,
    deletion: Option<&delete::Flow>,
) {
    if let Some(creation) = creation.filter(|flow| flow.blocks_input()) {
        creation.render(frame, frame.area());
        draw_test_server_banner(frame, model);
        return;
    }
    if model.mode() == Mode::Control {
        draw_control(frame, model, creation);
        if let Some(error) = creation_error {
            draw_creation_error(frame, error);
        }
        if let Some(deletion) = deletion {
            deletion.render(frame, frame.area());
        }
        if let Some(creation) = creation {
            creation.render_progress(frame, frame.area());
        }
        draw_test_server_banner(frame, model);
        return;
    }
    let screen = screen.expect("world mode requires a world screen");
    let world = world_area(frame.area());
    frame.render_widget(TerminalView(screen), world);
    draw_world_bar(frame, model);
    if let Some(message) = closed_message {
        draw_closed_session_bar(frame, message);
    }
    if let Some(creation) = creation {
        creation.render_progress(frame, frame.area());
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
    draw_test_server_banner(frame, model);
}

fn draw_test_server_banner(frame: &mut Frame<'_>, model: &ShellModel) {
    if !model.test_server() {
        return;
    }
    let area = frame.area();
    let label = " WT E2E TEST SERVER ";
    let width = u16::try_from(label.len())
        .unwrap_or(u16::MAX)
        .min(area.width);
    frame.render_widget(
        Paragraph::new(label).alignment(Alignment::Center).style(
            Style::new()
                .fg(Color::Yellow)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(area.right().saturating_sub(width), area.y, width, 1),
    );
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
        "↑ ctrl F6: close "
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
        Style::new()
            .fg(Color::DarkGray)
            .bg(Color::Black)
            .add_modifier(Modifier::BOLD)
    };
    let bar = Rect::new(frame.area().x, frame.area().y, frame.area().width, 1);
    let [previous, world, next] = super::bar::world_bar_controls(model, bar);
    let left = Rect::new(bar.x, bar.y, previous.x.saturating_sub(bar.x), 1);
    let right = Rect::new(
        next.right(),
        bar.y,
        bar.right().saturating_sub(next.right()),
        1,
    );
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
        left,
    );
    frame.render_widget(Paragraph::new("← ").style(style), previous);
    frame.render_widget(
        Paragraph::new(super::bar::world_bar_label(model))
            .alignment(Alignment::Center)
            .style(style),
        world,
    );
    frame.render_widget(Paragraph::new(" →").style(style), next);
    frame.render_widget(
        Paragraph::new(right_hint)
            .alignment(Alignment::Right)
            .style(style),
        right,
    );
}

fn draw_control(frame: &mut Frame<'_>, model: &ShellModel, creation: Option<&Flow>) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), area);
    let (activity_bar, content) = control_areas(area);
    draw_activity_bar(frame, activity_bar, model.control().activity());
    let (body, footer) = control_content_areas(area);
    match model.control().activity() {
        Activity::Worlds => draw_worlds(frame, body, model, creation),
        Activity::Codex => draw_codex(frame, body, model.control()),
    }
    let hint = match (model.control().activity(), model.has_worlds()) {
        (Activity::Worlds, true) => {
            "[ ↑/↓ or wheel: select ] [ Enter/click: open ] [ Tab: activity ] [ F5: world ]"
        }
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
    if model.control().open_failed() || model.control().context_failure().is_some() {
        draw_codex_toast(frame, area, model.control());
    }
}

fn draw_codex_toast(frame: &mut Frame<'_>, area: Rect, state: &ControlState) {
    let toast = super::toast::area(area);
    let (retry, _) = super::toast::actions(area);
    let (title, message) = if state.open_failed() {
        (
            " Could not open Codex session ".to_owned(),
            "The session could not be focused. Try again.".to_owned(),
        )
    } else {
        let contexts = state
            .context_failure()
            .expect("Codex toast has an open or context failure");
        let target = if contexts.len() == 1 {
            format!("context {}", contexts[0])
        } else {
            format!("{} contexts", contexts.len())
        };
        (
            format!(" Could not refresh Codex sessions for {target} "),
            "The context could not be queried. Try again.".to_owned(),
        )
    };
    frame.render_widget(Clear, toast);
    frame.render_widget(
        Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Red))
            .title(title)
            .title(
                Line::styled(
                    "×",
                    Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Right),
            ),
        toast,
    );
    frame.render_widget(
        Paragraph::new(message),
        Rect::new(
            toast.x.saturating_add(1),
            toast.y.saturating_add(1),
            toast.width.saturating_sub(2),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new("Retry")
            .alignment(Alignment::Right)
            .style(Style::new().fg(Color::White).add_modifier(Modifier::BOLD)),
        retry,
    );
}

fn draw_worlds(frame: &mut Frame<'_>, area: Rect, model: &ShellModel, creation: Option<&Flow>) {
    let block = Block::new()
        .borders(Borders::ALL)
        .title(refresh_title("Worlds", model.control().worlds_updated_at()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let creating = creation
        .and_then(Flow::creating_world)
        .filter(|(name, _)| model.worlds().iter().all(|world| world.name != *name));
    if !model.has_worlds() && creating.is_none() {
        frame.render_widget(
            Paragraph::new("No worlds with SSH access\nCreate a world to get started")
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }
    let count = model.world_count() + usize::from(creating.is_some());
    for (index, rect) in world_card_rects(frame.area(), model.active(), count) {
        if let Some((name, resources)) = creating.filter(|_| index == model.world_count()) {
            draw_world_card(
                frame,
                rect,
                "󰔟",
                Color::Yellow,
                "PROVISIONING",
                name,
                resources,
                None,
                false,
                "Creation in progress",
            );
            continue;
        }
        let world = &model.worlds()[index];
        let (icon, color) = match world.status {
            wt_control_protocol::InstanceStatus::Running => ("󰐊", Color::Green),
            wt_control_protocol::InstanceStatus::Provisioning => ("󰔟", Color::Yellow),
            wt_control_protocol::InstanceStatus::Stopped => ("󰅖", Color::DarkGray),
            wt_control_protocol::InstanceStatus::Destroying => ("󰩹", Color::Yellow),
            wt_control_protocol::InstanceStatus::Error => ("󰅚", Color::Red),
        };
        draw_world_card(
            frame,
            rect,
            icon,
            color,
            &world.status.to_string().to_uppercase(),
            &world.name,
            &world.resources,
            (world.detail != "-").then_some(world.detail.as_str()),
            index == model.active(),
            "Enter or click to open",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_world_card(
    frame: &mut Frame<'_>,
    area: Rect,
    icon: &str,
    color: Color,
    status: &str,
    name: &str,
    resources: &str,
    detail: Option<&str>,
    selected: bool,
    footer: &str,
) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(if selected {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            format!(" {icon} {status} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![
        Line::from(name.to_owned()),
        Line::from(resources.to_owned()),
    ];
    if let Some(detail) = detail {
        lines.push(Line::from(detail.to_owned()));
    }
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
    frame.render_widget(
        Paragraph::new(footer).style(Style::new().fg(Color::DarkGray)),
        rows[1],
    );
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

    let lines = card_lines(card);
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
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[1]);
}

fn card_title(card: &CodexCard) -> (String, Color) {
    let suffix = card
        .timestamp
        .map(relative_age)
        .map_or_else(String::new, |age| format!(" · {age}"));
    match &card.kind {
        CodexCardKind::Observation {
            state,
            session_start_source,
            ..
        } => {
            let (icon, label, color) = match state {
                wt_control_protocol::CodexSessionState::NeedsAttention => {
                    ("󰚩", "NEEDS ATTENTION".into(), Color::Yellow)
                }
                wt_control_protocol::CodexSessionState::Working => {
                    ("󰔟", "WORKING".into(), Color::Green)
                }
                wt_control_protocol::CodexSessionState::Unknown => (
                    "󰋗",
                    session_start_source
                        .as_ref()
                        .map_or_else(|| "UNKNOWN".into(), |source| format!("UNKNOWN ({source})")),
                    Color::Gray,
                ),
                wt_control_protocol::CodexSessionState::Inactive => {
                    ("󰅖", "INACTIVE".into(), Color::DarkGray)
                }
            };
            (format!("{icon} {label}{suffix}"), color)
        }
        CodexCardKind::RolloutOnly => (format!("󰈙 SAVED SESSION{suffix}"), Color::DarkGray),
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
            repository_root,
            repository_url,
            git_branch,
            target,
            ..
        } => {
            let git = repository_root.as_ref().map(|root| {
                let repository = repository_url
                    .as_deref()
                    .and_then(repository_name)
                    .or_else(|| std::path::Path::new(root).file_name()?.to_str())
                    .unwrap_or(root);
                git_branch.as_ref().map_or_else(
                    || format!("{repository} · {cwd}"),
                    |branch| format!("{repository} · {branch} · {cwd}"),
                )
            });
            vec![
                Line::from(
                    card.title
                        .clone()
                        .unwrap_or_else(|| "Untitled Codex session".into()),
                ),
                Line::from(git.unwrap_or_else(|| cwd.clone())),
                Line::from(format!(
                    "{}.{} · {}:{} · session {}",
                    card.context,
                    world_name,
                    target.tmux_session,
                    target.pane_id,
                    short_session.expect("observation card has session ID")
                )),
            ]
        }
        CodexCardKind::RolloutOnly => vec![
            Line::from(
                card.title
                    .clone()
                    .unwrap_or_else(|| "Untitled Codex session".into()),
            ),
            Line::from(format!(
                "{} · session {}",
                card.context,
                short_session.expect("rollout card has session ID")
            )),
            Line::from("Saved in Codex history, but not open in a WT pane"),
        ],
        CodexCardKind::ContextError { message } => vec![
            Line::from(format!("Context {}", card.context)),
            Line::styled(message.clone(), Style::new().fg(Color::Red)),
        ],
    }
}

fn repository_name(url: &str) -> Option<&str> {
    url.trim_end_matches(".git")
        .rsplit(['/', ':'])
        .find(|part| !part.is_empty())
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
#[path = "render_tests.rs"]
mod tests;
