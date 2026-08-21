use crossterm::event::KeyCode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    World,
    Switcher,
    Control,
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
}

impl ShellModel {
    pub(super) fn new(worlds: Vec<String>) -> Self {
        assert!(!worlds.is_empty(), "shell requires at least one world");
        Self {
            worlds,
            active: 0,
            mode: Mode::World,
        }
    }

    pub(super) fn mode(&self) -> Mode {
        self.mode
    }

    pub(super) fn active(&self) -> usize {
        self.active
    }

    pub(super) fn handle_key(&mut self, key: KeyCode) -> InputRoute {
        match self.mode {
            Mode::World if key == KeyCode::F(5) => {
                self.mode = Mode::Switcher;
                InputRoute::Consumed
            }
            Mode::World => InputRoute::World,
            Mode::Switcher => {
                match key {
                    KeyCode::F(5) => self.mode = Mode::World,
                    KeyCode::Left => {
                        self.active = self.active.checked_sub(1).unwrap_or(self.worlds.len() - 1);
                    }
                    KeyCode::Right => self.active = (self.active + 1) % self.worlds.len(),
                    KeyCode::Up => self.mode = Mode::Control,
                    _ => {}
                }
                InputRoute::Consumed
            }
            Mode::Control => {
                if key == KeyCode::F(5) {
                    self.mode = Mode::World;
                }
                InputRoute::Consumed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ShellModel {
        ShellModel::new(vec!["one".into(), "two".into(), "three".into()])
    }

    #[test]
    fn world_mode_forwards_every_key_except_f5() {
        let mut model = model();

        assert_eq!(model.handle_key(KeyCode::Left), InputRoute::World);
        assert_eq!(model.active(), 0);
        assert_eq!(model.handle_key(KeyCode::F(5)), InputRoute::Consumed);
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn switcher_cycles_worlds_without_leaving_the_overlay() {
        let mut model = model();
        model.handle_key(KeyCode::F(5));

        model.handle_key(KeyCode::Left);
        assert_eq!(model.active(), 2);
        model.handle_key(KeyCode::Right);
        model.handle_key(KeyCode::Right);
        assert_eq!(model.active(), 1);
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn up_opens_control_and_f5_closes_it() {
        let mut model = model();
        model.handle_key(KeyCode::F(5));
        model.handle_key(KeyCode::Up);

        assert_eq!(model.mode(), Mode::Control);
        assert_eq!(model.handle_key(KeyCode::Left), InputRoute::Consumed);
        assert_eq!(model.active(), 0);
        model.handle_key(KeyCode::F(5));
        assert_eq!(model.mode(), Mode::World);
    }

    #[test]
    fn f5_closes_the_switcher() {
        let mut model = model();
        model.handle_key(KeyCode::F(5));
        model.handle_key(KeyCode::F(5));

        assert_eq!(model.mode(), Mode::World);
    }
}
