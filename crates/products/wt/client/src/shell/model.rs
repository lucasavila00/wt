use super::control::{CodexOpenTarget, ControlAction, ControlCommand, ControlState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use uuid::Uuid;
use wt_control_protocol::{InstanceName, InstanceStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldIdentity {
    pub(super) context: String,
    pub(super) id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShellWorld {
    pub(super) identity: WorldIdentity,
    pub(super) name: String,
    pub(super) instance_name: InstanceName,
    pub(super) control_alias: String,
    pub(super) status: InstanceStatus,
    pub(super) resources: String,
    pub(super) detail: String,
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
            instance_name: InstanceName::parse(
                name.split_once('.').map_or(name, |(_, instance)| instance),
            )
            .unwrap(),
            control_alias: format!("{name}-direct"),
            status: InstanceStatus::Running,
            resources: "2 CPU · 4G · 1G/32G disk".into(),
            detail: "-".into(),
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
    test_server: bool,
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
            test_server: false,
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

    #[cfg(test)]
    pub(super) fn worlds_mut(&mut self) -> &mut [ShellWorld] {
        &mut self.worlds
    }

    pub(super) fn world_index(&self, identity: &WorldIdentity) -> Option<usize> {
        self.worlds
            .iter()
            .position(|world| &world.identity == identity)
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
    pub(super) fn control_mut(&mut self) -> &mut ControlState {
        &mut self.control
    }

    pub(super) fn show_worlds(&mut self) {
        self.control.show_worlds();
    }

    pub(super) fn finish_worlds_refresh(&mut self, result: Result<String, Vec<String>>) {
        self.control.finish_worlds_refresh(result);
    }

    #[cfg(test)]
    pub(super) fn set_codex(
        &mut self,
        codex: Vec<super::control::CodexCard>,
        updated_at: String,
        area: Rect,
    ) -> bool {
        self.control.set_codex(codex, updated_at, area)
    }

    #[cfg(test)]
    pub(super) fn set_codex_context_failures(&mut self, contexts: Vec<String>) {
        self.control.set_context_failures(contexts);
    }

    pub(super) fn resize(&mut self, area: Rect) {
        self.control.resize(area);
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> InputRoute {
        if key.code == KeyCode::F(6) {
            self.should_quit = true;
            return InputRoute::Consumed;
        }
        if self.mode == Mode::Switcher && self.control.palette().is_open() {
            return self
                .control
                .handle_key(key, area)
                .map_or(InputRoute::Consumed, route);
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
                KeyCode::Char('1') | KeyCode::F(1) if key.modifiers == KeyModifiers::NONE => {
                    self.control.handle_key(key, area);
                    InputRoute::Consumed
                }
                _ => InputRoute::World,
            },
            Mode::Control => {
                if key.code == KeyCode::F(5) && self.has_worlds() {
                    self.control.close();
                    self.mode = Mode::World;
                } else {
                    if self.control.palette().is_open() {
                        return self
                            .control
                            .handle_key(key, area)
                            .map_or(InputRoute::Consumed, route);
                    }
                    if self.has_worlds()
                        && self.control.activity() == super::control::Activity::Worlds
                    {
                        if key.modifiers == KeyModifiers::NONE
                            && self.move_world_grid_selection(key.code)
                        {
                            return InputRoute::Consumed;
                        }
                        if key.code == KeyCode::Enter && key.modifiers == KeyModifiers::NONE {
                            self.control.close();
                            self.mode = Mode::World;
                            return InputRoute::Consumed;
                        }
                    }
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
        if self.control.palette().is_open() {
            let (changed, action) = self.control.handle_mouse(mouse, area);
            return (changed, action.map(route));
        }
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && mouse.row == area.y
            && mouse.column >= area.x
            && mouse.column < area.right()
            && self.has_worlds()
            && self.mode != Mode::Control
        {
            if self.f5_disabled {
                self.f5_disabled = false;
                self.mode = Mode::Switcher;
            } else if self.mode == Mode::Switcher {
                let [previous, world, next] = super::bar::world_bar_controls(self, area);
                let brand = super::bar::world_bar_brand(area);
                let control = super::bar::world_bar_control(area, next);
                if previous.contains((mouse.column, mouse.row).into()) {
                    self.active = self.active.checked_sub(1).unwrap_or(self.worlds.len() - 1);
                } else if next.contains((mouse.column, mouse.row).into()) {
                    self.active = (self.active + 1) % self.worlds.len();
                } else if brand.contains((mouse.column, mouse.row).into())
                    || world.contains((mouse.column, mouse.row).into())
                    || control.contains((mouse.column, mouse.row).into())
                {
                    self.mode = Mode::Control;
                } else {
                    self.mode = Mode::World;
                }
            } else {
                self.mode = Mode::Switcher;
            }
            return (true, Some(InputRoute::Consumed));
        }
        if self.mode != Mode::Control {
            return (false, None);
        }
        let (changed, action) = self.control.handle_mouse(mouse, area);
        if !changed
            && self.has_worlds()
            && self.control.activity() == super::control::Activity::Worlds
        {
            match mouse.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    self.active = self.active.saturating_sub(2);
                    return (true, Some(InputRoute::Consumed));
                }
                crossterm::event::MouseEventKind::ScrollDown => {
                    self.active = (self.active + 2).min(self.worlds.len() - 1);
                    return (true, Some(InputRoute::Consumed));
                }
                _ => {}
            }
        }
        if !changed && self.control.activity() == super::control::Activity::Worlds {
            let Some(index) = super::control::world_card_at_position(
                area,
                self.active,
                self.worlds.len(),
                mouse.column,
                mouse.row,
            ) else {
                return (changed, action.map(route));
            };
            self.active = index;
            if mouse.kind
                == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
            {
                self.mode = Mode::World;
            }
            return (true, Some(InputRoute::Consumed));
        }
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
        target: &CodexOpenTarget,
        world: Option<usize>,
        failed: bool,
    ) {
        let accepted = self.control.finish_open(target, failed);
        if accepted
            && self.mode == Mode::Control
            && self.control.activity() != super::control::Activity::Worlds
            && !failed
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
#[path = "model_focus_tests.rs"]
mod focus_tests;

#[cfg(test)]
#[path = "model_navbar_tests.rs"]
mod navbar_tests;
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
    fn world_cards_select_and_open_worlds() {
        let mut model = ShellModel::new(vec![world("one"), world("two"), world("three")]);
        model.handle_key(key(KeyCode::Tab), area());
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
    fn command_palette_executes_from_the_worlds_activity() {
        let mut model = ShellModel::new(vec![world("one")]);
        model.handle_key(key(KeyCode::Tab), area());
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
    fn clicking_the_world_bar_activates_it_and_clicking_arrows_changes_worlds() {
        let mut model = model();

        assert!(model.handle_mouse(mouse(0, 0), area()).0);
        assert_eq!(model.mode(), Mode::Switcher);
        let [previous, _, _] = super::super::bar::world_bar_controls(&model, area());
        assert_eq!(previous.width, 7);
        model.handle_mouse(mouse(previous.right() - 1, previous.y), area());
        assert_eq!(model.active(), 2);
        let [_, _, next] = super::super::bar::world_bar_controls(&model, area());
        assert_eq!(next.width, 7);
        model.handle_mouse(mouse(next.x, next.y), area());
        assert_eq!(model.active(), 0);
    }

    #[test]
    fn clicking_a_bold_control_target_opens_the_control_ui() {
        for target in ["brand", "world", "control"] {
            let mut model = model();
            model.handle_mouse(mouse(0, 0), area());
            let [_, world, next] = super::super::bar::world_bar_controls(&model, area());
            let brand = super::super::bar::world_bar_brand(area());
            let control = super::super::bar::world_bar_control(area(), next);
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
    fn clicking_a_disabled_world_bar_restores_the_override() {
        let mut model = model();
        model.handle_key(shifted(KeyCode::F(5)), area());

        assert!(model.f5_disabled());
        assert!(model.handle_mouse(mouse(0, 0), area()).0);
        assert!(!model.f5_disabled());
        assert_eq!(model.mode(), Mode::Switcher);
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

    fn mouse(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }
}
