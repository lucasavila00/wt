use super::control::{
    card_grid, control_areas, control_content_areas, Activity, PaneCard, PaneCardKind,
    WORLD_CARD_HEIGHT,
};
use super::delete;
use super::model::{Mode, ShellModel};
use super::terminal_view::TerminalView;
use super::world_area;
use crate::create::Flow;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    frame: &mut Frame<'_>,
    screens: &[&vt100::Screen],
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
        draw_control(frame, model, creation);
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
    super::control_overlay::draw_palette(frame, world, model.control().palette());
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
fn draw_control(frame: &mut Frame<'_>, model: &ShellModel, creation: Option<&Flow>) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let (activity_bar, content) = control_areas(area);
    super::activity::draw(frame, activity_bar, model.control().activity());
    let (body, footer) = control_content_areas(area);
    match model.control().activity() {
        Activity::Worlds => draw_worlds(frame, body, model, creation),
        Activity::Live => super::live::draw(frame, body, model),
    }
    let (title, failure) = match model.control().activity() {
        Activity::Worlds => {
            let status = model.control().worlds_refresh();
            (status.title("Worlds"), status.failure())
        }
        Activity::Live => {
            let status = model.control().pane_refresh();
            (status.title("Live Codex screens"), status.failure())
        }
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
    let mut title = vec![Span::styled(title, muted_style())];
    if let Some(failure) = failure {
        title.push(Span::styled(failure, Style::new().fg(Color::Red)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(title)).style(muted_style()),
        title_area,
    );
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
    super::control_overlay::draw_palette(frame, content, model.control().palette());
    super::control_overlay::draw_world_menu(frame, model);
    super::control_overlay::draw_help(frame, content, model);
}

fn draw_worlds(frame: &mut Frame<'_>, area: Rect, model: &ShellModel, creation: Option<&Flow>) {
    let state = model.control();
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
    let grid = card_grid(frame.area(), state.world_scroll(), count, WORLD_CARD_HEIGHT);
    super::scrollbar::render(frame, grid, muted_style());
    for card in grid.cards() {
        let index = card.index;
        if let Some((name, resources)) = creating.filter(|_| index == model.world_count()) {
            grid.render_card(frame, card, |rect, buffer| {
                super::world_card::draw(
                    buffer,
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
                    false,
                )
            });
            continue;
        }
        let world = &model.worlds()[index];
        let idle = world.status == wt_control_protocol::WorldStatus::Running
            && model.control().pane_refresh().updated_at().is_some()
            && model.control().pane_refresh().failures().is_none()
            && super::world_card::is_idle(world, model.control().panes());
        let (icon, color, status) = super::world_card::status(world, idle);
        grid.render_card(frame, card, |rect, buffer| {
            super::world_card::draw(
                buffer,
                rect,
                icon,
                color,
                &status,
                &world.name,
                &world.resources,
                (world.detail != "-").then_some(world.detail.as_str()),
                &[
                    super::world_card::git_lines(world),
                    super::world_card::pane_lines(world, model.control().panes()),
                ]
                .concat(),
                index == model.active(),
                "",
                true,
            )
        });
    }
}

pub(super) fn card_title(card: &PaneCard) -> (String, Color) {
    match &card.kind {
        PaneCardKind::Observation { .. } => {
            if card.is_stale() {
                let suffix = format!(" · {}", relative_age(card.timestamp()));
                (format!("󰅚 STALE{suffix}"), Color::Yellow)
            } else if card.changed_recently() {
                ("󰔟 CHANGING".into(), Color::Green)
            } else {
                let suffix = format!(" · {}", relative_age(card.timestamp()));
                (format!("󰚩 STATIC{suffix}"), Color::Yellow)
            }
        }
        PaneCardKind::ContextError => ("󰅚 CONTEXT ERROR".into(), Color::Red),
    }
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

pub(super) fn muted_style() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

pub(super) fn selected_card_border_style(selected: bool) -> Style {
    if selected {
        Style::new().fg(Color::Blue)
    } else {
        Style::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::control::{PaneCardIdentity, PaneCardKind};

    #[test]
    fn changing_pane_titles_omit_the_relative_age() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap();
        let card = PaneCard {
            identity: PaneCardIdentity::Observation {
                context: "ars".into(),
                world_id: uuid::Uuid::nil().into(),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
            },
            context: "ars".into(),
            created_at_unix_ms: Some(now),
            observed_at_unix_ms: Some(now),
            kind: PaneCardKind::Observation {
                world_name: "dev".into(),
                changed_at_unix_ms: now,
                cwd: "/home/wt/wt".into(),
                git_branch: Some("wt/live-pane-cwd".into()),
                render: wt_control_protocol::PaneRender {
                    window_index: 0,
                    window_name: "codex".into(),
                    frame: wt_control_protocol::PaneFrame {
                        rows: 1,
                        columns: 1,
                        cells: vec![wt_control_protocol::PaneCell {
                            text: "C".into(),
                            foreground: wt_control_protocol::PaneColor::Default,
                            background: wt_control_protocol::PaneColor::Default,
                            bold: false,
                            italic: false,
                            underlined: false,
                            inverse: false,
                        }],
                    },
                },
            },
        };

        insta::assert_snapshot!(card_title(&card).0, @"󰔟 CHANGING");
    }
}
