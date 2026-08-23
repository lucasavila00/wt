use super::control::{command_palette_layout, CommandPalette};
use super::model::ShellModel;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
};
use ratatui::Frame;

pub(super) fn draw_palette(frame: &mut Frame<'_>, content: Rect, palette: &CommandPalette) {
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
        Paragraph::new("─".repeat(usize::from(rows[1].width))).style(super::render::muted_style()),
        rows[1],
    );
    let commands = palette.matches();
    let items = if commands.is_empty() {
        vec![ListItem::new("No matching commands").style(super::render::muted_style())]
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
        Paragraph::new("↑/↓ select · Enter run · Esc close").style(super::render::muted_style()),
        rows[3],
    );
}

pub(super) fn draw_world_menu(frame: &mut Frame<'_>, model: &ShellModel) {
    let Some(menu) = model.world_menu() else {
        return;
    };
    let Some(index) = model.world_index(menu.identity()) else {
        return;
    };
    menu.render(frame, &model.worlds()[index].name);
}

pub(super) fn draw_help(frame: &mut Frame<'_>, content: Rect, model: &ShellModel) {
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
        Paragraph::new("Esc: close").style(super::render::muted_style()),
        sections[1],
    );
}

pub(super) fn draw_codex_toast(frame: &mut Frame<'_>, area: Rect) {
    let toast = super::toast::area(area);
    let (retry, _) = super::toast::actions(area);
    frame.render_widget(Clear, toast);
    frame.render_widget(
        Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Red))
            .title(" Could not open Codex session ")
            .title(
                Line::styled("×", Style::new().add_modifier(Modifier::BOLD))
                    .alignment(Alignment::Right),
            ),
        toast,
    );
    frame.render_widget(
        Paragraph::new("The session could not be focused. Try again."),
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
