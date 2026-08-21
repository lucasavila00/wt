use super::control::{
    CodexCard, CodexCardIdentity, CodexOpenTarget, ControlAction, ControlCommand, ControlState,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
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
    pub(super) world_name: String,
    pub(super) kind: wt_control_protocol::WorldKind,
    pub(super) control_alias: String,
}

#[cfg(test)]
impl From<&str> for ShellWorld {
    fn from(name: &str) -> Self {
        Self {
            identity: WorldIdentity {
                context: name
                    .split_once('.')
                    .map_or("local", |(context, _)| context)
                    .into(),
                id: Uuid::new_v4(),
            },
            name: name.into(),
            world_name: name
                .rsplit_once('.')
                .map_or(name, |(_, world)| world)
                .into(),
            kind: wt_control_protocol::WorldKind::Host,
            control_alias: format!("{name}-vs"),
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
    f5_disabled: bool,
    control: ControlState,
    should_quit: bool,
}

impl ShellModel {
    pub(super) fn new(worlds: Vec<ShellWorld>) -> Self {
        Self {
            worlds,
            active: 0,
            mode: Mode::Control,
            f5_disabled: false,
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

    pub(super) fn f5_disabled(&self) -> bool {
        self.f5_disabled
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

    pub(super) fn worlds(&self) -> &[ShellWorld] {
        &self.worlds
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
            self.f5_disabled = false;
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

    pub(super) fn set_worlds_updated_at(&mut self, updated_at: String) {
        self.control.set_worlds_updated_at(updated_at);
    }

    pub(super) fn set_codex(
        &mut self,
        codex: Vec<CodexCard>,
        updated_at: String,
        area: Rect,
    ) -> bool {
        self.control.set_codex(codex, updated_at, area)
    }

    pub(super) fn resize(&mut self, area: Rect) {
        self.control.resize(area);
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> InputRoute {
        if key.code == KeyCode::F(6) {
            self.should_quit = true;
            return InputRoute::Consumed;
        }
        if key.code == KeyCode::F(5) && key.modifiers == KeyModifiers::SHIFT && self.has_worlds() {
            self.f5_disabled = !self.f5_disabled;
            if self.f5_disabled {
                self.control.close();
                self.mode = Mode::World;
            }
            return InputRoute::Consumed;
        }
        if key.code == KeyCode::F(5) && self.f5_disabled {
            return InputRoute::World;
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
            .find(|(_, world)| {
                world.identity.context == target.context && world.identity.id == target.world_id
            })
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
    use uuid::Uuid;
    use wt_control_protocol::{ByobuTarget, CodexSessionState};

    fn model() -> ShellModel {
        let mut model = ShellModel::new(vec![world("one"), world("two"), world("three")]);
        model.handle_key(key(KeyCode::F(5)), area());
        model
    }

    fn world(name: &str) -> ShellWorld {
        ShellWorld::test(name, Uuid::new_v4().as_u128())
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn shifted(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
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
    fn shift_f5_disables_the_override_and_plain_f5_reaches_the_world() {
        let mut model = model();

        assert_eq!(
            model.handle_key(shifted(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert!(model.f5_disabled());
        assert_eq!(model.mode(), Mode::World);
        assert_eq!(
            model.handle_key(key(KeyCode::F(5)), area()),
            InputRoute::World
        );

        assert_eq!(
            model.handle_key(shifted(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert!(!model.f5_disabled());
        assert_eq!(
            model.handle_key(key(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.mode(), Mode::Switcher);
    }

    #[test]
    fn shell_starts_in_control_mode() {
        let model = ShellModel::new(vec![world("one")]);

        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn empty_shell_stays_in_control_mode_on_f5() {
        let mut model = ShellModel::new(Vec::new());

        assert_eq!(
            model.handle_key(key(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert_eq!(
            model.handle_key(shifted(KeyCode::F(5)), area()),
            InputRoute::Consumed
        );
        assert!(!model.f5_disabled());
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
        model.set_codex(
            vec![CodexCard {
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
            }],
            "2026-08-21T20:00:00Z".into(),
            area(),
        );
        model
    }

    fn open_codex_activity(model: &mut ShellModel) {
        for code in [KeyCode::F(5), KeyCode::Up] {
            model.handle_key(key(code), area());
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
        assert!(!model.f5_disabled());
        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn world_identity_includes_the_context() {
        let id = Uuid::new_v4();
        let local = ShellWorld::test("local.same", id.as_u128());
        let lab = ShellWorld::test("lab.same", id.as_u128());
        let model = ShellModel::new(vec![local.clone(), lab.clone()]);

        assert_eq!(model.world_index(&local.identity), Some(0));
        assert_eq!(model.world_index(&lab.identity), Some(1));
    }

    fn area() -> Rect {
        Rect::new(0, 0, 80, 24)
    }
}
