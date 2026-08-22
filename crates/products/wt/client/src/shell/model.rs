use super::control::{
    CodexCard, CodexCardIdentity, CodexOpenTarget, ControlAction, ControlCommand, ControlState,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use uuid::Uuid;
use wt_control_protocol::InstanceName;
use wt_control_protocol::InstanceStatus;

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

    pub(super) fn show_worlds(&mut self) {
        self.control.show_worlds();
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
                    if self.has_worlds()
                        && self.control.activity() == super::control::Activity::Worlds
                    {
                        match key.code {
                            KeyCode::Up if key.modifiers == KeyModifiers::NONE => {
                                self.active = self.active.saturating_sub(1);
                                return InputRoute::Consumed;
                            }
                            KeyCode::Down if key.modifiers == KeyModifiers::NONE => {
                                self.active = (self.active + 1).min(self.worlds.len() - 1);
                                return InputRoute::Consumed;
                            }
                            KeyCode::Enter if key.modifiers == KeyModifiers::NONE => {
                                self.control.close();
                                self.mode = Mode::World;
                                return InputRoute::Consumed;
                            }
                            _ => {}
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
                let [previous, _, next] = super::bar::world_bar_controls(self, area);
                if previous.contains((mouse.column, mouse.row).into()) {
                    self.active = self.active.checked_sub(1).unwrap_or(self.worlds.len() - 1);
                } else if next.contains((mouse.column, mouse.row).into()) {
                    self.active = (self.active + 1) % self.worlds.len();
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
                    self.active = self.active.saturating_sub(3);
                    return (true, Some(InputRoute::Consumed));
                }
                crossterm::event::MouseEventKind::ScrollDown => {
                    self.active = (self.active + 3).min(self.worlds.len() - 1);
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
#[path = "model/tests.rs"]
mod tests;
