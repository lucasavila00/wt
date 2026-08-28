use super::control::{ControlCommand, ControlState, PaneCardIdentity};
use super::world_menu::WorldMenu;
use crossterm::event::KeyCode;
#[cfg(test)]
use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use wt_control_protocol::{WorldId, WorldName, WorldStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldIdentity {
    pub(super) context: String,
    pub(super) world_id: WorldId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShellWorld {
    pub(super) identity: WorldIdentity,
    pub(super) name: String,
    pub(super) world_name: WorldName,
    pub(super) control_alias: String,
    pub(super) status: WorldStatus,
    pub(super) resources: String,
    pub(super) detail: String,
    pub(super) action_log: super::action_log::ActionLog,
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
                world_id: WorldId::new(),
            },
            name: name.into(),
            world_name: WorldName::parse(name.split_once('.').map_or(name, |(_, world)| world))
                .unwrap(),
            control_alias: format!("{name}-direct"),
            status: WorldStatus::Running,
            resources: "2 CPU · 4G · 1G/32G disk".into(),
            detail: "-".into(),
            action_log: super::action_log::ActionLog::Loading,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    World,
    Control,
}

impl Mode {
    pub(super) fn forwards_mouse(self) -> bool {
        matches!(self, Self::World)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum InputRoute {
    Consumed,
    World,
    Command(ControlCommand),
    OpenPane(Box<PaneCardIdentity>),
    DeleteWorld(Box<ShellWorld>),
}

#[derive(Debug)]
pub(super) struct ShellModel {
    worlds: Vec<ShellWorld>,
    active: usize,
    mode: Mode,
    test_server: bool,
    control: ControlState,
    should_quit: bool,
    world_menu: Option<WorldMenu>,
}

impl ShellModel {
    pub(super) fn new(worlds: Vec<ShellWorld>) -> Self {
        Self {
            worlds,
            active: 0,
            mode: Mode::Control,
            test_server: false,
            control: ControlState::default(),
            should_quit: false,
            world_menu: None,
        }
    }

    pub(super) fn mode(&self) -> Mode {
        self.mode
    }

    pub(super) fn active(&self) -> usize {
        self.active
    }

    pub(super) fn test_server(&self) -> bool {
        self.test_server
    }

    pub(super) fn set_test_server(&mut self, test_server: bool) {
        self.test_server = test_server;
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

    pub(super) fn world_menu(&self) -> Option<&WorldMenu> {
        self.world_menu.as_ref()
    }

    pub(super) fn reconcile_worlds(&mut self, mut worlds: Vec<ShellWorld>) {
        let active_identity = self
            .worlds
            .get(self.active)
            .map(|world| world.identity.clone());
        for world in &mut worlds {
            if let Some(existing) = self
                .worlds
                .iter()
                .find(|existing| existing.identity == world.identity)
            {
                world.action_log = existing.action_log.clone();
            }
        }
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
        if self
            .world_menu
            .as_ref()
            .is_some_and(|menu| self.world_index(menu.identity()).is_none())
        {
            self.world_menu = None;
        }
    }

    pub(super) fn apply_action_log(&mut self, update: super::action_log::WorldActionLog) -> bool {
        let Some(world) = self.worlds.iter_mut().find(|world| {
            world.identity.context == update.context && world.identity.world_id == update.world_id
        }) else {
            return false;
        };
        let activity = match update.actions {
            Some(actions) => super::action_log::ActionLog::Loaded(actions),
            None if matches!(world.action_log, super::action_log::ActionLog::Loading) => {
                super::action_log::ActionLog::Unavailable
            }
            None => return false,
        };
        if world.action_log == activity {
            return false;
        }
        world.action_log = activity;
        true
    }

    pub(super) fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub(super) fn control(&self) -> &ControlState {
        &self.control
    }
    pub(super) fn control_mut(&mut self) -> &mut ControlState {
        &mut self.control
    }

    pub(super) fn show_worlds(&mut self) {
        self.control.show_worlds();
    }

    pub(super) fn finish_worlds_refresh(&mut self, result: Result<String, Vec<String>>) {
        self.control.finish_worlds_refresh(result);
    }

    pub(super) fn resize(&mut self, area: Rect) {
        self.control.resize(area);
        self.control
            .keep_world_selection_visible(area, self.active, self.worlds.len());
    }

    pub(super) fn pane_route(&self, target: &PaneCardIdentity) -> Option<(usize, &str)> {
        if !self
            .control
            .panes()
            .iter()
            .any(|card| &card.identity == target)
        {
            return None;
        }
        let PaneCardIdentity::Observation {
            context, world_id, ..
        } = target
        else {
            return None;
        };
        self.worlds
            .iter()
            .enumerate()
            .find(|(_, world)| {
                world.identity.context == *context && world.identity.world_id == *world_id
            })
            .map(|(index, world)| (index, world.control_alias.as_str()))
    }

    pub(super) fn open_world(&mut self, index: usize) {
        self.active = index;
        self.control.close();
        self.mode = Mode::World;
    }
}

#[cfg(test)]
#[path = "model_world_menu_tests.rs"]
mod world_menu_tests;

mod input;
mod world_grid;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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
        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn shift_f5_opens_control_instead_of_disabling_f5() {
        let mut model = model();

        assert_eq!(
            model.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::SHIFT), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.mode(), Mode::Control);
    }

    #[test]
    fn world_cards_select_and_open_worlds() {
        let mut model = ShellModel::new(vec![world("one"), world("two"), world("three")]);
        model.handle_key(key(KeyCode::Tab), area());

        model.handle_key(key(KeyCode::Down), area());
        assert_eq!(model.active_world(), "three");
        model.handle_key(key(KeyCode::Left), area());
        assert_eq!(model.active_world(), "two");
        assert_eq!(model.mode(), Mode::Control);

        model.handle_key(key(KeyCode::Enter), area());
        assert_eq!(model.active_world(), "two");
        assert_eq!(model.mode(), Mode::World);
    }

    #[test]
    fn provisioning_card_uses_the_same_scroll_bounds_as_rendering() {
        let mut model = ShellModel::new(vec![
            world("one"),
            world("two"),
            world("three"),
            world("four"),
        ]);
        model.show_worlds();
        let scroll = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 20,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };

        assert!(model.handle_mouse_with_world_count(scroll, area(), 5).0);
        assert_eq!(model.control().world_scroll(), 1);
    }

    #[test]
    fn command_palette_executes_from_the_worlds_activity() {
        let mut model = ShellModel::new(vec![world("one")]);
        model.handle_key(key(KeyCode::Tab), area());
        model.handle_key(key(KeyCode::F(1)), area());
        for character in "delete".chars() {
            model.handle_key(key(KeyCode::Char(character)), area());
        }

        assert_eq!(
            model.handle_key(key(KeyCode::Enter), area()),
            InputRoute::Command(ControlCommand::DeleteWorld)
        );
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
    fn world_mode_forwards_navigation_keys_to_the_world() {
        let mut model = model();

        assert_eq!(
            model.handle_key(key(KeyCode::Left), area()),
            InputRoute::World
        );
        assert_eq!(
            model.handle_key(key(KeyCode::Right), area()),
            InputRoute::World
        );
        assert_eq!(model.active(), 0);
        assert_eq!(model.mode(), Mode::World);
    }

    #[test]
    fn f5_opens_control_and_closes_the_palette() {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)), area());
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
    fn world_mode_forwards_mouse_to_the_world() {
        assert!(Mode::World.forwards_mouse());
        assert!(!Mode::Control.forwards_mouse());
    }

    #[test]
    fn clicking_a_bold_control_target_opens_the_control_ui() {
        for target in ["brand", "world", "control"] {
            let mut model = model();
            let world = super::super::bar::world_bar_world(&model, area());
            let brand = super::super::bar::world_bar_brand(area());
            let control = super::super::bar::world_bar_control(area());
            let target = match target {
                "brand" => brand,
                "world" => world,
                "control" => control,
                _ => unreachable!(),
            };

            model.handle_mouse(mouse(target.x, target.y), area());

            assert_eq!(model.mode(), Mode::Control);
        }
    }

    #[test]
    fn f6_closes_from_every_mode_without_forwarding() {
        for mode in [Mode::World, Mode::Control] {
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
    fn action_log_preserves_loaded_actions_across_failures_and_inventory_refreshes() {
        let mut model = ShellModel::new(vec![world("one")]);
        let identity = model.worlds()[0].identity.clone();
        let actions = vec!["Git: pushed wt/topic to github.com/owner/repository".into()];
        assert!(
            model.apply_action_log(super::super::action_log::WorldActionLog {
                context: identity.context.clone(),
                world_id: identity.world_id,
                actions: Some(actions.clone()),
            })
        );
        assert!(
            !model.apply_action_log(super::super::action_log::WorldActionLog {
                context: identity.context.clone(),
                world_id: identity.world_id,
                actions: None,
            })
        );

        let mut refreshed = model.worlds()[0].clone();
        refreshed.action_log = super::super::action_log::ActionLog::Loading;
        model.reconcile_worlds(vec![refreshed]);

        assert_eq!(
            model.worlds()[0].action_log,
            super::super::action_log::ActionLog::Loaded(actions)
        );
    }

    #[test]
    fn initial_action_log_failure_is_not_reported_as_empty_activity() {
        let mut model = ShellModel::new(vec![world("one")]);
        let identity = model.worlds()[0].identity.clone();

        assert!(
            model.apply_action_log(super::super::action_log::WorldActionLog {
                context: identity.context,
                world_id: identity.world_id,
                actions: None,
            })
        );
        assert_eq!(
            model.worlds()[0].action_log,
            super::super::action_log::ActionLog::Unavailable
        );
    }

    #[test]
    fn reconciliation_opens_control_when_all_worlds_are_removed() {
        let mut model = model();

        model.reconcile_worlds(Vec::new());

        assert!(!model.has_worlds());
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

    fn mouse(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }
}
