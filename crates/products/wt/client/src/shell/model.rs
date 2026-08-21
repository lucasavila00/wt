use super::control::{CodexContextSnapshot, ControlState};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    World,
    Switcher,
    Control,
}

impl Mode {
    pub(super) fn forwards_mouse(self) -> bool {
        matches!(self, Self::World | Self::Switcher)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InputRoute {
    Consumed,
    World,
}

#[derive(Debug)]
pub(super) struct ShellModel {
    worlds: Vec<String>,
    active: usize,
    mode: Mode,
    control: ControlState,
    should_quit: bool,
}

impl ShellModel {
    pub(super) fn new(worlds: Vec<String>) -> Self {
        assert!(!worlds.is_empty(), "shell requires at least one world");
        Self {
            worlds,
            active: 0,
            mode: Mode::World,
            control: ControlState::default(),
            should_quit: false,
        }
    }

    pub(super) fn mode(&self) -> Mode {
        self.mode
    }

    pub(super) fn active(&self) -> usize {
        self.active
    }

    pub(super) fn active_world(&self) -> &str {
        &self.worlds[self.active]
    }

    pub(super) fn world_count(&self) -> usize {
        self.worlds.len()
    }

    pub(super) fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub(super) fn control(&self) -> &ControlState {
        &self.control
    }

    pub(super) fn set_codex(&mut self, codex: Vec<CodexContextSnapshot>) {
        self.control.set_codex(codex);
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> InputRoute {
        if key.code == KeyCode::F(6) {
            self.should_quit = true;
            return InputRoute::Consumed;
        }
        match self.mode {
            Mode::World if key.code == KeyCode::F(5) => {
                self.mode = Mode::Switcher;
                InputRoute::Consumed
            }
            Mode::World => InputRoute::World,
            Mode::Switcher => match key.code {
                KeyCode::F(5) => {
                    self.mode = Mode::World;
                    InputRoute::Consumed
                }
                KeyCode::Left => {
                    self.active = self.active.checked_sub(1).unwrap_or(self.worlds.len() - 1);
                    InputRoute::Consumed
                }
                KeyCode::Right => {
                    self.active = (self.active + 1) % self.worlds.len();
                    InputRoute::Consumed
                }
                KeyCode::Up => {
                    self.mode = Mode::Control;
                    InputRoute::Consumed
                }
                _ => InputRoute::World,
            },
            Mode::Control => {
                if key.code == KeyCode::F(5) {
                    self.control.close();
                    self.mode = Mode::World;
                } else {
                    self.control.handle_key(key);
                }
                InputRoute::Consumed
            }
        }
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> bool {
        self.mode == Mode::Control && self.control.handle_mouse(mouse, area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn model() -> ShellModel {
        ShellModel::new(vec!["one".into(), "two".into(), "three".into()])
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn world_mode_forwards_every_key_except_f5() {
        let mut model = model();

        assert_eq!(model.handle_key(key(KeyCode::Left)), InputRoute::World);
        assert_eq!(model.active(), 0);
        assert_eq!(model.handle_key(key(KeyCode::F(5))), InputRoute::Consumed);
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn switcher_cycles_worlds_without_leaving_the_bar() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)));

        assert_eq!(model.handle_key(key(KeyCode::Left)), InputRoute::Consumed);
        assert_eq!(model.active(), 2);
        assert_eq!(model.handle_key(key(KeyCode::Right)), InputRoute::Consumed);
        assert_eq!(model.handle_key(key(KeyCode::Right)), InputRoute::Consumed);
        assert_eq!(model.active(), 1);
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn switcher_forwards_unadvertised_keys_to_the_world() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)));

        assert_eq!(model.handle_key(key(KeyCode::Char('x'))), InputRoute::World);
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn up_opens_control_and_f5_closes_it() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)));
        assert_eq!(model.handle_key(key(KeyCode::Up)), InputRoute::Consumed);

        assert_eq!(model.mode(), Mode::Control);
        assert_eq!(model.handle_key(key(KeyCode::Left)), InputRoute::Consumed);
        assert_eq!(model.active(), 0);
        model.handle_key(key(KeyCode::F(1)));
        assert!(model.control().palette().is_open());
        assert_eq!(model.handle_key(key(KeyCode::F(5))), InputRoute::Consumed);
        assert_eq!(model.mode(), Mode::World);
        assert!(!model.control().palette().is_open());
    }

    #[test]
    fn f5_closes_the_switcher() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)));
        model.handle_key(key(KeyCode::F(5)));

        assert_eq!(model.mode(), Mode::World);
    }

    #[test]
    fn switcher_forwards_mouse_to_the_world() {
        assert!(Mode::World.forwards_mouse());
        assert!(Mode::Switcher.forwards_mouse());
        assert!(!Mode::Control.forwards_mouse());
    }

    #[test]
    fn f6_closes_from_every_mode_without_forwarding() {
        for mode in [Mode::World, Mode::Switcher, Mode::Control] {
            let mut model = model();
            model.mode = mode;

            assert_eq!(model.handle_key(key(KeyCode::F(6))), InputRoute::Consumed);
            assert!(model.should_quit());
        }
    }
}
