use super::ControlCommand;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(in crate::shell) const COMMANDS: [ControlCommand; 2] =
    [ControlCommand::NewWorld, ControlCommand::DeleteWorld];

#[derive(Debug, Default)]
pub(in crate::shell) struct CommandPalette {
    open: bool,
    query: String,
    selected: usize,
}

impl CommandPalette {
    pub(in crate::shell) fn is_open(&self) -> bool {
        self.open
    }

    pub(in crate::shell) fn query(&self) -> &str {
        &self.query
    }

    pub(in crate::shell) fn selected(&self) -> usize {
        self.selected
    }

    pub(in crate::shell) fn matches(&self) -> Vec<ControlCommand> {
        let query = self.query.to_ascii_lowercase();
        COMMANDS
            .iter()
            .copied()
            .filter(|command| command.label().to_ascii_lowercase().contains(&query))
            .collect()
    }

    pub(in crate::shell) fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    pub(in crate::shell) fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
    }

    pub(in crate::shell) fn handle_key(&mut self, key: KeyEvent) -> Option<ControlCommand> {
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
            KeyCode::Enter => return self.execute(self.selected),
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

    pub(in crate::shell) fn execute(&mut self, index: usize) -> Option<ControlCommand> {
        let command = self.matches().get(index).copied();
        self.close();
        command
    }
}
