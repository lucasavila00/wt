use super::control::{
    codex_card_rects, command_palette_layout, control_areas, control_content_areas,
    world_card_rects, Activity, CodexCard, CodexCardKind, CommandPalette, ControlState,
};
use super::delete;
use super::model::{Mode, ShellModel};
use super::terminal_view::TerminalView;
use super::world_area;
use crate::create::Flow;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
};
use ratatui::Frame;

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    frame: &mut Frame<'_>,
    screens: &[&vt100::Screen],
    live_focus: &super::live_focus::LiveFocus,
    closed_message: Option<&str>,
    model: &ShellModel,
    creation: Option<&Flow>,
    action_error: Option<&str>,
    deletion: Option<&delete::Flow>,
) {
    if model.mode() == Mode::Control {
        if let Some(creation) = creation.filter(|flow| flow.blocks_input()) {
            creation.render(frame, frame.area());
            draw_test_server_banner(frame, model);
            return;
        }
    }
    if model.mode() == Mode::Control {
        draw_control(frame, screens, live_focus, model, creation);
        if let Some(error) = action_error {
            draw_action_error(frame, error);
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
    let screen = screens[model.active()];
    let world = world_area(frame.area());
    frame.render_widget(TerminalView(screen), world);
    draw_world_bar(frame, model);
    if let Some(message) = closed_message {
        draw_closed_session_bar(frame, message);
    }
    if let Some(creation) = creation {
        if creation.blocks_input() {
            creation.render_overlay(frame, frame.area());
        } else {
            creation.render_progress(frame, frame.area());
        }
    }
    draw_command_palette(frame, world, model.control().palette());
    if let Some(error) = action_error {
        draw_action_error(frame, error);
    }
    if let Some(deletion) = deletion {
        deletion.render(frame, frame.area());
    }
    match model.mode() {
        Mode::World if closed_message.is_none() => {
            if !screen.hide_cursor() {
                let (row, column) = screen.cursor_position();
                frame.set_cursor_position((world.x + column, world.y + row));
            }
        }
        Mode::World => {}
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
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED)),
        Rect::new(area.right().saturating_sub(width), area.y, width, 1),
    );
}
fn draw_closed_session_bar(frame: &mut Frame<'_>, message: &str) {
    let area = frame.area();
    frame.render_widget(
        Paragraph::new(format!(" {message} · Space: reconnect "))
            .alignment(Alignment::Center)
            .style(Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED)),
        Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
    );
}
fn draw_action_error(frame: &mut Frame<'_>, error: &str) {
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
                    .title("Action failed")
                    .title_bottom(" Enter/Esc close "),
            ),
        area,
    );
}
fn draw_world_bar(frame: &mut Frame<'_>, model: &ShellModel) {
    let style = Style::new().add_modifier(Modifier::DIM);
    let clickable_style = style.add_modifier(Modifier::BOLD);
    let bar = Rect::new(frame.area().x, frame.area().y, frame.area().width, 1);
    let world = super::bar::world_bar_world(model, bar);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            super::bar::BRAND_LABEL,
            clickable_style,
        )]))
        .style(style),
        bar,
    );
    frame.render_widget(
        Paragraph::new(super::bar::world_bar_label(model))
            .alignment(Alignment::Center)
            .style(clickable_style),
        world,
    );
    let right_hint = Line::from(vec![
        Span::styled(super::bar::CONTROL_LABEL, clickable_style),
        Span::raw(super::bar::CLOSE_LABEL),
    ]);
    frame.render_widget(
        Paragraph::new(right_hint)
            .alignment(Alignment::Right)
            .style(style),
        bar,
    );
}
fn draw_control(
    frame: &mut Frame<'_>,
    screens: &[&vt100::Screen],
    live_focus: &super::live_focus::LiveFocus,
    model: &ShellModel,
    creation: Option<&Flow>,
) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let (activity_bar, content) = control_areas(area);
    super::activity::draw(frame, activity_bar, model.control().activity());
    let (body, footer) = control_content_areas(area);
    match model.control().activity() {
        Activity::Worlds => draw_worlds(frame, body, model, creation),
        Activity::Codex => draw_codex(frame, body, model.control()),
        Activity::Live => super::live::draw(frame, body, screens, live_focus, model),
    }
    let title = match model.control().activity() {
        Activity::Worlds => model.control().worlds_refresh().title("Worlds"),
        Activity::Codex => model.control().codex_refresh().title("Codex sessions"),
        Activity::Live => "Live sessions · Experimental".to_owned(),
    };
    let capacity = wt_client::inventory::format_capacity(model.control().capacity());
    let help = super::control::help_control_area(footer);
    let capacity_width = capacity.as_ref().map_or(0, |text| {
        u16::try_from(text.chars().count() + 1).unwrap_or(u16::MAX)
    });
    let [title_area, resources, help_area] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(capacity_width),
        Constraint::Length(help.width),
    ])
    .areas(footer);
    frame.render_widget(Paragraph::new(title).style(muted_style()), title_area);
    if let Some(capacity) = capacity {
        frame.render_widget(
            Paragraph::new(capacity)
                .alignment(Alignment::Right)
                .style(muted_style()),
            resources,
        );
    }
    frame.render_widget(
        Paragraph::new(super::control::HELP_CONTROL)
            .alignment(Alignment::Right)
            .style(Style::new().add_modifier(Modifier::BOLD)),
        help_area,
    );
    draw_command_palette(frame, content, model.control().palette());
    draw_help(frame, content, model);
    if model.control().open_failed() {
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
                Line::styled("×", Style::new().add_modifier(Modifier::BOLD))
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
            .style(Style::new().add_modifier(Modifier::BOLD)),
        retry,
    );
}

fn draw_help(frame: &mut Frame<'_>, content: Rect, model: &ShellModel) {
    let help = model.control().help();
    if !help.is_open() {
        return;
    }
    let width = 64.min(content.width);
    let height = 14.min(content.height);
    let area = Rect::new(
        content.x + content.width.saturating_sub(width) / 2,
        content.y + content.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    let block = Block::new().borders(Borders::ALL).title(" Help ");
    let inner = block.inner(area).inner(Margin::new(2, 1));
    frame.render_widget(block, area);
    let sections = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let rows = help
        .rows(model.control().activity(), model.has_worlds())
        .into_iter()
        .map(|(shortcut, action)| Row::new([Cell::from(shortcut), Cell::from(action)]));
    frame.render_widget(
        Table::new(rows, [Constraint::Length(20), Constraint::Min(0)])
            .header(Row::new(["Shortcut", "Action"]))
            .column_spacing(2),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new("Esc: close").style(muted_style()),
        sections[1],
    );
}

fn draw_worlds(frame: &mut Frame<'_>, area: Rect, model: &ShellModel, creation: Option<&Flow>) {
    let creating = creation
        .and_then(Flow::creating_world)
        .filter(|(name, _)| model.worlds().iter().all(|world| world.name != *name));
    if !model.has_worlds() && creating.is_none() {
        frame.render_widget(
            Paragraph::new("No worlds with SSH access\nCreate a world to get started")
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    let count = model.world_count() + usize::from(creating.is_some());
    super::scrollbar::render_world_cards(frame, count, model.active(), muted_style());
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
                &[],
                false,
                "Creation in progress",
            );
            continue;
        }
        let world = &model.worlds()[index];
        let (icon, color) = match world.status {
            wt_control_protocol::InstanceStatus::Running => ("󰐊", Color::Green),
            wt_control_protocol::InstanceStatus::Provisioning => ("󰔟", Color::Yellow),
            wt_control_protocol::InstanceStatus::Stopped => ("󰅖", Color::Reset),
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
            &super::world_card::codex_lines(world, model.control().codex()),
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
    codex: &[Line<'static>],
    selected: bool,
    footer: &str,
) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(selected_card_border_style(selected))
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
    lines.extend_from_slice(codex);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
    frame.render_widget(Paragraph::new(footer).style(muted_style()), rows[1]);
}

fn draw_codex(frame: &mut Frame<'_>, area: Rect, state: &ControlState) {
    if state.codex().is_empty() {
        let message = if state.codex_refresh().updated_at().is_some() {
            "No Codex sessions\nStart Codex in a world to see its session here"
        } else {
            "Loading Codex sessions…"
        };
        frame.render_widget(Paragraph::new(message).alignment(Alignment::Center), area);
        return;
    }
    super::scrollbar::render_codex_cards(
        frame,
        state.codex().len(),
        state.codex_offset(),
        muted_style(),
    );
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
        .border_style(selected_card_border_style(selected))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(title_color).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let footer = if state.opening() == Some(&card.identity) {
        Span::styled("OPENING…", Style::new().fg(Color::Yellow))
    } else if let Some(reason) = card.disabled_reason() {
        Span::styled(format!("Unavailable: {reason}"), muted_style())
    } else {
        Span::styled("Enter or click to open", muted_style())
    };
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let metadata = card_metadata_lines(card);
    if let Some(preview) = card.latest_user_message.as_deref() {
        let content_rows =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(rows[0]);
        let preview_paragraph = Paragraph::new(preview).wrap(Wrap { trim: false });
        let truncated = wrapped_line_count(preview, content_rows[0].width) > 3;
        frame.render_widget(preview_paragraph, content_rows[0]);
        if truncated && !content_rows[0].is_empty() {
            let ellipsis = Rect::new(
                content_rows[0].right().saturating_sub(1),
                content_rows[0].bottom().saturating_sub(1),
                1,
                1,
            );
            frame.render_widget(Paragraph::new("…"), ellipsis);
        }
        frame.render_widget(Paragraph::new(metadata), content_rows[1]);
    } else {
        frame.render_widget(Paragraph::new(metadata), rows[0]);
    }
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[1]);
}
pub(super) fn card_title(card: &CodexCard) -> (String, Color) {
    let suffix = card
        .timestamp
        .map(relative_age)
        .map_or_else(String::new, |age| format!(" · {age}"));
    match &card.kind {
        CodexCardKind::Observation {
            state,
            is_compacting,
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
                    ("󰅖", "INACTIVE".into(), Color::Reset)
                }
            };
            let compacting = is_compacting.then_some(" (COMPACTING)").unwrap_or_default();
            (format!("{icon} {label}{compacting}{suffix}"), color)
        }
        CodexCardKind::RolloutOnly => (format!("󰈙 SAVED SESSION{suffix}"), Color::Reset),
        CodexCardKind::ContextError { .. } => ("󰅚 CONTEXT ERROR".into(), Color::Red),
    }
}
fn card_metadata_lines(card: &CodexCard) -> Vec<Line<'static>> {
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

pub(super) fn repository_name(url: &str) -> Option<&str> {
    url.trim_end_matches(".git")
        .rsplit(['/', ':'])
        .find(|part| !part.is_empty())
}

fn wrapped_line_count(value: &str, width: u16) -> usize {
    let width = usize::from(width);
    if width == 0 {
        return 0;
    }
    let mut lines = 1;
    let mut used = 0;
    for word in value.split_whitespace() {
        let word_width = Line::from(word).width();
        if used > 0 && used + 1 + word_width <= width {
            used += 1 + word_width;
            continue;
        }
        if used > 0 {
            lines += 1;
        }
        lines += word_width.saturating_sub(1) / width;
        used = word_width % width;
        if used == 0 && word_width > 0 {
            used = width;
        }
    }
    lines
}

pub(super) fn relative_age(timestamp: i64) -> String {
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
        Paragraph::new("─".repeat(usize::from(rows[1].width))).style(muted_style()),
        rows[1],
    );
    let commands = palette.matches();
    let items = if commands.is_empty() {
        vec![ListItem::new("No matching commands").style(muted_style())]
    } else {
        commands
            .iter()
            .map(|command| ListItem::new(command.label()))
            .collect()
    };
    let list = List::new(items)
        .highlight_symbol(" ")
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(
        (!commands.is_empty()).then_some(palette.selected().min(commands.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(list, rows[2], &mut state);
    frame.render_widget(
        Paragraph::new("↑/↓ select · Enter run · Esc close").style(muted_style()),
        rows[3],
    );
}

pub(super) fn muted_style() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

pub(super) fn selected_card_border_style(selected: bool) -> Style {
    if selected {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new()
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "render_extra_tests.rs"]
mod extra_tests;
#[cfg(test)]
use extra_tests::now_ms;
