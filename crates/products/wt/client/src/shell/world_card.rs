use super::model::ShellWorld;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

pub(super) fn status(world: &ShellWorld, idle: bool) -> (&'static str, Color, String) {
    match (world.status, idle) {
        (_, true) => (
            "󰚩",
            Color::Yellow,
            "STATIC · NO RECENT PANE CHANGE".to_owned(),
        ),
        (wt_control_protocol::WorldStatus::Running, false) => {
            ("󰐊", Color::Green, "RUNNING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Provisioning, false) => {
            ("󰔟", Color::Yellow, "PROVISIONING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Stopped, false) => {
            ("󰅖", Color::Reset, "STOPPED".to_owned())
        }
        (wt_control_protocol::WorldStatus::Destroying, false) => {
            ("󰩹", Color::Yellow, "DESTROYING".to_owned())
        }
        (wt_control_protocol::WorldStatus::Error, false) => ("󰅚", Color::Red, "ERROR".to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    buffer: &mut Buffer,
    area: Rect,
    icon: &str,
    color: Color,
    status: &str,
    name: &str,
    resources: &str,
    detail: Option<&str>,
    details: &[Line<'static>],
    selected: bool,
    footer: &str,
    show_actions: bool,
) {
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_style(super::render::selected_card_border_style(selected))
        .title(Span::styled(
            format!(" {icon} {status} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
    if show_actions {
        block = block.title(
            Line::styled(
                super::world_menu::CARD_LABEL,
                Style::new().add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Right),
        );
    }
    let inner = block.inner(area);
    block.render(area, buffer);
    let mut lines = vec![
        Line::from(name.to_owned()),
        Line::from(resources.to_owned()),
    ];
    if let Some(detail) = detail {
        lines.push(Line::from(detail.to_owned()));
    }
    lines.extend_from_slice(details);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(rows[0], buffer);
    Paragraph::new(footer)
        .style(super::render::muted_style())
        .render(rows[1], buffer);
}
