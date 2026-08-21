use super::codex::ShellWorld;
use super::control::{
    CodexCard, CodexCardIdentity, CodexOpenTarget, ControlAction, ControlCommand, ControlState,
};
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum InputRoute {
    Consumed,
    World,
    OpenCodex(Box<CodexOpenTarget>),
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
        &self.worlds[self.active].qualified_name
    }

    pub(super) fn world_count(&self) -> usize {
        self.worlds.len()
    }

    pub(super) fn world_index(&self, world: &str) -> Option<usize> {
        self.worlds
            .iter()
            .position(|candidate| candidate.qualified_name == world)
    }

    pub(super) fn activate_world(&mut self, world: ShellWorld) {
        self.active = match self.world_index(&world.qualified_name) {
            Some(index) => index,
            None => {
                self.worlds.push(world);
                self.worlds.len() - 1
            }
        };
        self.mode = Mode::World;
    }

    pub(super) fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub(super) fn control(&self) -> &ControlState {
        &self.control
    }

    pub(super) fn set_codex(&mut self, codex: Vec<CodexCard>) {
        self.control.set_codex(codex);
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> InputRoute {
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
                    if let Some(action) = self.control.handle_key(key, area) {
                        return route(action);
                    }
                }
                InputRoute::Consumed
            }
        }
    }

    pub(super) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
    ) -> (bool, Option<InputRoute>) {
        if self.mode != Mode::Control {
            return (false, None);
        }
        let (changed, action) = self.control.handle_mouse(mouse, area);
        (changed, action.map(route))
    }

    pub(super) fn focus_route(&self, target: &CodexOpenTarget) -> Option<(usize, &str)> {
        self.worlds
            .iter()
            .enumerate()
            .find(|(_, world)| world.context == target.context && world.world_id == target.world_id)
            .map(|(index, world)| (index, world.control_alias.as_str()))
    }

    pub(super) fn finish_codex_open(
        &mut self,
        identity: &CodexCardIdentity,
        world: Option<usize>,
        error: Option<String>,
    ) {
        let accepted = self.control.finish_open(identity, error.clone());
        if accepted
            && self.mode == Mode::Control
            && self.control.activity() == super::control::Activity::Codex
            && error.is_none()
        {
            let Some(world) = world else {
                return;
            };
            self.active = world;
            self.mode = Mode::World;
        }
    }
}

fn route(action: ControlAction) -> InputRoute {
    match action {
        ControlAction::Command(command) => InputRoute::Command(command),
        ControlAction::OpenCodex(target) => InputRoute::OpenCodex(target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use uuid::Uuid;
    use wt_control_protocol::{ByobuTarget, CodexSessionState};

    fn model() -> ShellModel {
        let mut model = ShellModel::new(vec![
            ShellWorld::test("one", 1),
            ShellWorld::test("two", 2),
            ShellWorld::test("three", 3),
        ]);
        model.handle_key(key(KeyCode::F(5)), area());
        model
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn world_mode_forwards_every_key_except_f5() {
        let mut model = model();

        assert_eq!(
            model.handle_key(key(KeyCode::Left), area()),
            InputRoute::World
        );
        assert_eq!(model.active(), 0);
        assert_eq!(
            model.handle_key(key(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn shell_starts_in_control_mode() {
        let model = ShellModel::new(vec![ShellWorld::test("one", 1)]);

        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn empty_shell_stays_in_control_mode_on_f5() {
        let mut model = ShellModel::new(Vec::new());

        assert_eq!(
            model.handle_key(key(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn switcher_cycles_worlds_without_leaving_the_bar() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)), area());

        assert_eq!(
            model.handle_key(key(KeyCode::Left), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.active(), 2);
        assert_eq!(
            model.handle_key(key(KeyCode::Right), area()),
            InputRoute::Consumed
        );
        assert_eq!(
            model.handle_key(key(KeyCode::Right), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.active(), 1);
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn switcher_forwards_unadvertised_keys_to_the_world() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)), area());

        assert_eq!(
            model.handle_key(key(KeyCode::Char('x')), area()),
            InputRoute::World
        );
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn up_opens_control_and_f5_closes_it() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)), area());
        assert_eq!(
            model.handle_key(key(KeyCode::Up), area()),
            InputRoute::Consumed
        );

        assert_eq!(model.mode(), Mode::Control);
        assert_eq!(
            model.handle_key(key(KeyCode::Left), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.active(), 0);
        model.handle_key(key(KeyCode::F(1)), area());
        assert!(model.control().palette().is_open());
        assert_eq!(
            model.handle_key(key(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.mode(), Mode::World);
        assert!(!model.control().palette().is_open());
    }

    #[test]
    fn f5_closes_the_switcher() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)), area());
        model.handle_key(key(KeyCode::F(5)), area());

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

            assert_eq!(
                model.handle_key(key(KeyCode::F(6)), area()),
                InputRoute::Consumed
            );
            assert!(model.should_quit());
        }
    }

    #[test]
    fn completed_focus_switches_only_from_the_active_codex_view() {
        let mut model = model_with_open_card();
        open_codex_activity(&mut model);
        let InputRoute::OpenCodex(target) = model.handle_key(key(KeyCode::Enter), area()) else {
            panic!("live card did not produce an open target");
        };
        model.finish_codex_open(&target.identity, Some(1), None);
        assert_eq!(model.active(), 1);
        assert_eq!(model.mode(), Mode::World);

        let mut canceled = model_with_open_card();
        open_codex_activity(&mut canceled);
        let InputRoute::OpenCodex(target) = canceled.handle_key(key(KeyCode::Enter), area()) else {
            panic!("live card did not produce an open target");
        };
        canceled.handle_key(key(KeyCode::F(5)), area());
        canceled.finish_codex_open(&target.identity, Some(1), None);
        assert_eq!(canceled.active(), 0);
        assert_eq!(canceled.mode(), Mode::World);
    }

    fn model_with_open_card() -> ShellModel {
        let mut model = model();
        let session_id = Uuid::from_u128(10);
        let world_id = Uuid::from_u128(2);
        let target = ByobuTarget {
            tmux_session: "wt-host".into(),
            pane_id: "%1".into(),
        };
        model.set_codex(vec![CodexCard {
            identity: CodexCardIdentity::Observation {
                context: "local".into(),
                session_id,
                world_id,
                tmux_session: target.tmux_session.clone(),
                pane_id: target.pane_id.clone(),
            },
            context: "local".into(),
            session_id: Some(session_id),
            timestamp: Some(1),
            kind: super::super::control::CodexCardKind::Observation {
                world_id,
                world_name: "two".into(),
                cwd: "/workspace".into(),
                state: CodexSessionState::Working,
                target,
            },
        }]);
        model
    }

    fn open_codex_activity(model: &mut ShellModel) {
        for code in [KeyCode::F(5), KeyCode::Up] {
            model.handle_key(key(code), area());
        }
    }

    fn area() -> Rect {
        Rect::new(0, 0, 80, 24)
    }
}
