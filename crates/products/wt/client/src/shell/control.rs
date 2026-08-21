use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Margin, Rect};

pub(super) const COMMANDS: [ControlCommand; 2] = [ControlCommand::NewHost, ControlCommand::NewDev];
pub(super) const ACTIVITY_BAR_WIDTH: u16 = 5;
pub(super) const ACTIVITY_BUTTON_HEIGHT: u16 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Activity {
    Worlds,
    Codex,
}

impl Activity {
    fn next(self) -> Self {
        match self {
            Self::Worlds => Self::Codex,
            Self::Codex => Self::Worlds,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ControlCommand {
    NewHost,
    NewDev,
}

impl ControlCommand {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::NewHost => "World: New host",
            Self::NewDev => "World: New dev",
        }
    }

    fn execute(self) {
        match self {
            Self::NewHost => {}
            Self::NewDev => {}
        }
    }
}

#[derive(Debug)]
pub(super) struct ControlState {
    activity: Activity,
    palette: CommandPalette,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            activity: Activity::Worlds,
            palette: CommandPalette::default(),
        }
    }
}

impl ControlState {
    pub(super) fn activity(&self) -> Activity {
        self.activity
    }

    pub(super) fn palette(&self) -> &CommandPalette {
        &self.palette
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        if self.palette.is_open() {
            if let Some(command) = self.palette.handle_key(key) {
                command.execute();
            }
            return;
        }
        if key.modifiers != KeyModifiers::NONE {
            return;
        }
        match key.code {
            KeyCode::Tab => self.activity = self.activity.next(),
            KeyCode::Char('1') | KeyCode::F(1) => self.palette.open(),
            _ => {}
        }
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        if self.palette.is_open() {
            let (_, results) = command_palette_layout(control_areas(area).1);
            if results.contains((mouse.column, mouse.row).into()) {
                let index = usize::from(mouse.row.saturating_sub(results.y));
                if index < self.palette.matches().len() {
                    if let Some(command) = self.palette.execute(index) {
                        command.execute();
                    }
                }
            }
            return true;
        }
        if let Some(activity) = activity_at_position(area, mouse.column, mouse.row) {
            self.activity = activity;
            return true;
        }
        false
    }

    pub(super) fn close(&mut self) {
        self.palette.close();
    }
}

#[derive(Debug, Default)]
pub(super) struct CommandPalette {
    open: bool,
    query: String,
    selected: usize,
}

impl CommandPalette {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn matches(&self) -> Vec<ControlCommand> {
        let query = self.query.to_ascii_lowercase();
        COMMANDS
            .iter()
            .copied()
            .filter(|command| command.label().to_ascii_lowercase().contains(&query))
            .collect()
    }

    fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<ControlCommand> {
        match key.code {
            KeyCode::Esc => self.close(),
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.matches().len().saturating_sub(1));
            }
            KeyCode::Enter => {
                return self.execute(self.selected);
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.selected = 0;
            }
            _ => {}
        }
        None
    }

    fn execute(&mut self, index: usize) -> Option<ControlCommand> {
        let command = self.matches().get(index).copied();
        self.close();
        command
    }
}

pub(super) fn control_areas(area: Rect) -> (Rect, Rect) {
    let columns = Layout::horizontal([Constraint::Length(ACTIVITY_BAR_WIDTH), Constraint::Min(0)])
        .split(area);
    (columns[0], columns[1])
}

pub(super) fn command_palette_layout(content: Rect) -> (Rect, Rect) {
    let width = (content.width.saturating_mul(70) / 100)
        .clamp(30.min(content.width), 70.min(content.width));
    let height = 9.min(content.height);
    let area = Rect::new(
        content.x + content.width.saturating_sub(width) / 2,
        content.y + content.height.saturating_mul(20) / 100,
        width,
        height,
    );
    let inner = area.inner(Margin::new(1, 1));
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    (area, rows[2])
}

fn activity_at_position(area: Rect, column: u16, row: u16) -> Option<Activity> {
    let (bar, _) = control_areas(area);
    if column < bar.x
        || column >= bar.right().saturating_sub(1)
        || row < bar.y
        || row >= bar.bottom()
    {
        return None;
    }
    match row.saturating_sub(bar.y) / ACTIVITY_BUTTON_HEIGHT {
        0 => Some(Activity::Worlds),
        1 => Some(Activity::Codex),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycles_activities() {
        let mut state = ControlState::default();

        state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.activity(), Activity::Codex);
        state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.activity(), Activity::Worlds);
    }

    #[test]
    fn f1_and_one_open_the_command_palette() {
        for code in [KeyCode::F(1), KeyCode::Char('1')] {
            let mut state = ControlState::default();
            state.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
            assert!(state.palette().is_open());
        }
    }

    #[test]
    fn palette_filters_selects_and_executes_no_op_commands() {
        let mut state = ControlState::default();
        state.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        for character in "dev".chars() {
            state.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }

        assert_eq!(state.palette().matches(), vec![ControlCommand::NewDev]);
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!state.palette().is_open());
    }

    #[test]
    fn activity_icons_and_palette_results_are_clickable() {
        let mut state = ControlState::default();
        let area = Rect::new(0, 0, 64, 16);

        assert!(state.handle_mouse(mouse(1, 4), area));
        assert_eq!(state.activity(), Activity::Codex);
        state.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        let (_, results) = command_palette_layout(control_areas(area).1);
        assert!(state.handle_mouse(mouse(results.x, results.y + 1), area));
        assert!(!state.palette().is_open());

        state.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        assert!(state.handle_mouse(mouse(results.x, results.y + 3), area));
        assert!(state.palette().is_open());
    }

    fn mouse(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }
}
