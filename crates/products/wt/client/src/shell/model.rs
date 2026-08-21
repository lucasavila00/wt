use super::control::{CodexContextSnapshot, ControlCommand, ControlState};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldIdentity {
    pub(super) context: String,
    pub(super) id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShellWorld {
    pub(super) identity: WorldIdentity,
    pub(super) name: String,
}

#[cfg(test)]
impl From<&str> for ShellWorld {
    fn from(name: &str) -> Self {
        Self {
            identity: WorldIdentity {
                context: name.split_once('.').map_or("local", |(context, _)| context).into(),
                id: Uuid::new_v4(),
            },
            name: name.into(),
        }
    }
}

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
    Command(ControlCommand),
}

#[derive(Debug)]
pub(super) struct ShellModel {
    worlds: Vec<ShellWorld>,
    active: usize,
    mode: Mode,
    control: ControlState,
    should_quit: bool,
}

impl ShellModel {
    pub(super) fn new(worlds: Vec<ShellWorld>) -> Self {
        Self {
            worlds,
            active: 0,
            mode: Mode::Control,
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

    pub(super) fn has_worlds(&self) -> bool {
        !self.worlds.is_empty()
    }

    pub(super) fn active_world(&self) -> &str {
        &self.worlds[self.active].name
    }

    pub(super) fn world_count(&self) -> usize {
        self.worlds.len()
    }

    pub(super) fn world_index(&self, identity: &WorldIdentity) -> Option<usize> {
        self.worlds
            .iter()
            .position(|world| &world.identity == identity)
    }

    pub(super) fn activate_world(&mut self, world: ShellWorld) {
        self.active = match self.world_index(&world.identity) {
            Some(index) => index,
            None => {
                self.worlds.push(world);
                self.worlds.len() - 1
            }
        };
        self.mode = Mode::World;
    }

    pub(super) fn reconcile_worlds(&mut self, worlds: Vec<ShellWorld>) {
        let active_identity = self
            .worlds
            .get(self.active)
            .map(|world| world.identity.clone());
        self.worlds = worlds;
        self.active = self
            .worlds
            .iter()
            .position(|world| Some(&world.identity) == active_identity.as_ref())
            .unwrap_or(0);
        if self.worlds.is_empty() {
            self.control.close();
            self.mode = Mode::Control;
        }
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
                if key.code == KeyCode::F(5) && self.has_worlds() {
                    self.control.close();
                    self.mode = Mode::World;
                } else {
                    if let Some(command) = self.control.handle_key(key) {
                        return InputRoute::Command(command);
                    }
                }
                InputRoute::Consumed
            }
        }
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> Option<ControlCommand> {
        (self.mode == Mode::Control)
            .then(|| self.control.handle_mouse(mouse, area))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn model() -> ShellModel {
        let mut model = ShellModel::new(vec![world("one"), world("two"), world("three")]);
        model.handle_key(key(KeyCode::F(5)));
        model
    }

    fn world(name: &str) -> ShellWorld {
        ShellWorld {
            identity: WorldIdentity {
                context: "local".into(),
                id: Uuid::new_v4(),
            },
            name: name.into(),
        }
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
    fn shell_starts_in_control_mode() {
        let model = ShellModel::new(vec!["one".into()]);

        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn empty_shell_stays_in_control_mode_on_f5() {
        let mut model = ShellModel::new(Vec::new());

        assert_eq!(model.handle_key(key(KeyCode::F(5))), InputRoute::Consumed);
        assert_eq!(model.mode(), Mode::Control);
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

    #[test]
    fn reconciliation_preserves_the_active_world_or_selects_the_first() {
        let mut model = model();
        model.active = 1;

        let active = model.worlds[1].clone();
        model.reconcile_worlds(vec![world("zero"), active, world("four")]);
        assert_eq!(model.active_world(), "two");

        model.reconcile_worlds(vec![world("four"), world("zero")]);
        assert_eq!(model.active_world(), "four");
    }

    #[test]
    fn reconciliation_opens_control_when_all_worlds_are_removed() {
        let mut model = model();

        model.reconcile_worlds(Vec::new());

        assert!(!model.has_worlds());
        assert_eq!(model.mode(), Mode::Control);
    }
}
