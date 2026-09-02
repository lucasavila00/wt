use super::control::{command_palette_layout, control_areas};
use super::model::WorldIdentity;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub(super) const CARD_LABEL: &str = " … Menu ";

#[derive(Debug)]
pub(super) struct WorldMenu {
    identity: WorldIdentity,
    selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MenuAction {
    None,
    Close,
    Messages,
    Delete,
}

impl WorldMenu {
    pub(super) fn new(identity: WorldIdentity) -> Self {
        Self {
            identity,
            selected: 0,
        }
    }

    pub(super) fn identity(&self) -> &WorldIdentity {
        &self.identity
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> MenuAction {
        if key.modifiers != KeyModifiers::NONE {
            return MenuAction::None;
        }
        match key.code {
            KeyCode::Esc => MenuAction::Close,
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                MenuAction::None
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(1);
                MenuAction::None
            }
            KeyCode::Enter if self.selected == 0 => MenuAction::Messages,
            KeyCode::Enter => MenuAction::Delete,
            _ => MenuAction::None,
        }
    }

    pub(super) fn handle_mouse(&self, mouse: MouseEvent, area: Rect) -> MenuAction {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return MenuAction::None;
        }
        let results = menu_result_area(area);
        if mouse.column < results.x || mouse.column >= results.right() {
            return MenuAction::None;
        }
        match mouse.row.checked_sub(results.y) {
            Some(0) => MenuAction::Messages,
            Some(1) => MenuAction::Delete,
            _ => MenuAction::None,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame<'_>, world_name: &str) {
        let (area, world, separator, results, footer) = menu_layout(frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(Block::new().borders(Borders::ALL).title("World Menu"), area);
        frame.render_widget(Paragraph::new(world_name.to_owned()), world);
        frame.render_widget(
            Paragraph::new("─".repeat(usize::from(separator.width)))
                .style(super::render::muted_style()),
            separator,
        );
        let list = List::new([ListItem::new("Messages"), ListItem::new("Delete")])
            .highlight_symbol(" ")
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(list, results, &mut state);
        frame.render_widget(
            Paragraph::new("Enter run · Esc close").style(super::render::muted_style()),
            footer,
        );
    }
}

pub(super) fn menu_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let content = control_areas(area).1;
    let (modal, results) = command_palette_layout(content);
    let inner = modal.inner(Margin::new(1, 1));
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    (modal, rows[0], rows[1], results, rows[3])
}

pub(super) fn menu_result_area(area: Rect) -> Rect {
    menu_layout(area).3
}
