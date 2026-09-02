use super::{InputRoute, Mode, ShellModel};
use crate::shell::control::{Activity, ControlAction};
use crate::shell::world_menu::{MenuAction, WorldMenu, CARD_LABEL};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

impl ShellModel {
    pub(in crate::shell) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> InputRoute {
        if key.code == KeyCode::F(6) {
            self.should_quit = true;
            return InputRoute::Consumed;
        }
        match self.mode {
            Mode::World if key.code == KeyCode::F(5) => {
                self.mode = Mode::Control;
                InputRoute::Consumed
            }
            Mode::World => InputRoute::World,
            Mode::Control => self.handle_control_key(key, area),
        }
    }

    fn handle_control_key(&mut self, key: KeyEvent, area: Rect) -> InputRoute {
        if let Some(mailbox) = self.mailbox.as_mut() {
            if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
                self.mailbox = None;
            } else {
                let _ = mailbox.handle_key(key);
            }
            return InputRoute::Consumed;
        }
        if key.code == KeyCode::F(5) && self.has_worlds() {
            self.control.close();
            self.world_menu = None;
            self.mode = Mode::World;
            return InputRoute::Consumed;
        }
        if let Some(menu) = &mut self.world_menu {
            let action = menu.handle_key(key);
            let identity = menu.identity().clone();
            return self.apply_menu_action(action, &identity);
        }
        if self.control.palette().is_open() {
            return self
                .control
                .handle_key(key, area)
                .map_or(InputRoute::Consumed, route);
        }
        if self.has_worlds() && self.control.activity() == Activity::Worlds {
            if key.modifiers == KeyModifiers::NONE && self.move_world_grid_selection(key.code) {
                self.control
                    .keep_world_selection_visible(area, self.active, self.worlds.len());
                return InputRoute::Consumed;
            }
            if key.code == KeyCode::Enter && key.modifiers == KeyModifiers::NONE {
                self.control.close();
                self.mode = Mode::World;
                return InputRoute::Consumed;
            }
        }
        self.control
            .handle_key(key, area)
            .map_or(InputRoute::Consumed, route)
    }

    pub(in crate::shell) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
    ) -> (bool, Option<InputRoute>) {
        self.handle_mouse_with_world_count(mouse, area, self.worlds.len())
    }

    pub(in crate::shell) fn handle_mouse_with_world_count(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        world_card_count: usize,
    ) -> (bool, Option<InputRoute>) {
        if self.mailbox.is_some() {
            return (true, Some(InputRoute::Consumed));
        }
        if let Some(menu) = &self.world_menu {
            let action = menu.handle_mouse(mouse, area);
            let identity = menu.identity().clone();
            return (true, Some(self.apply_menu_action(action, &identity)));
        }
        if self.control.palette().is_open() {
            let (changed, action) = self.control.handle_mouse(mouse, area);
            return (changed, action.map(route));
        }
        if self.world_bar_clicked(mouse, area) {
            return (true, Some(InputRoute::Consumed));
        }
        if self.mode != Mode::Control {
            return (false, None);
        }
        if self.control.activity() == Activity::Worlds
            && self
                .control
                .handle_world_scrollbar(mouse, area, world_card_count)
        {
            return (true, Some(InputRoute::Consumed));
        }
        let (changed, action) = self.control.handle_mouse(mouse, area);
        if !changed && self.has_worlds() && self.control.activity() == Activity::Worlds {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.control.scroll_worlds(-1, area, world_card_count);
                    return (true, Some(InputRoute::Consumed));
                }
                MouseEventKind::ScrollDown => {
                    self.control.scroll_worlds(1, area, world_card_count);
                    return (true, Some(InputRoute::Consumed));
                }
                _ => {}
            }
        }
        if !changed && self.control.activity() == Activity::Worlds {
            return self.handle_world_card_mouse(mouse, area, changed, action);
        }
        (changed, action.map(route))
    }

    fn apply_menu_action(
        &mut self,
        action: MenuAction,
        identity: &super::WorldIdentity,
    ) -> InputRoute {
        match action {
            MenuAction::None => InputRoute::Consumed,
            MenuAction::Close => {
                self.world_menu = None;
                InputRoute::Consumed
            }
            MenuAction::Delete => {
                self.world_menu = None;
                self.world_index(identity)
                    .map(|index| InputRoute::DeleteWorld(Box::new(self.worlds[index].clone())))
                    .unwrap_or(InputRoute::Consumed)
            }
            MenuAction::Messages => {
                self.world_menu = None;
                self.world_index(identity)
                    .map(|index| InputRoute::ShowMessages(Box::new(self.worlds[index].clone())))
                    .unwrap_or(InputRoute::Consumed)
            }
        }
    }

    fn world_bar_clicked(&mut self, mouse: MouseEvent, area: Rect) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left)
            || mouse.row != area.y
            || mouse.column < area.x
            || mouse.column >= area.right()
            || !self.has_worlds()
            || self.mode != Mode::World
        {
            return false;
        }
        let position = (mouse.column, mouse.row).into();
        if crate::shell::bar::world_bar_brand(area).contains(position)
            || crate::shell::bar::world_bar_world(self, area).contains(position)
            || crate::shell::bar::world_bar_control(area).contains(position)
        {
            self.mode = Mode::Control;
        }
        true
    }

    fn handle_world_card_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        changed: bool,
        action: Option<ControlAction>,
    ) -> (bool, Option<InputRoute>) {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let action_width = u16::try_from(CARD_LABEL.chars().count()).unwrap_or(u16::MAX);
            if let Some(index) = crate::shell::control::world_card_action_at_position(
                area,
                self.control.world_scroll(),
                self.worlds.len(),
                action_width,
                mouse.column,
                mouse.row,
            ) {
                self.active = index;
                self.world_menu = Some(WorldMenu::new(self.worlds[index].identity.clone()));
                return (true, Some(InputRoute::Consumed));
            }
        }
        let Some(index) = crate::shell::control::world_card_at_position(
            area,
            self.control.world_scroll(),
            self.worlds.len(),
            mouse.column,
            mouse.row,
        ) else {
            return (changed, action.map(route));
        };
        self.active = index;
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            self.mode = Mode::World;
        }
        (true, Some(InputRoute::Consumed))
    }
}

fn route(action: ControlAction) -> InputRoute {
    match action {
        ControlAction::Command(command) => InputRoute::Command(command),
        ControlAction::OpenPane(identity) => InputRoute::OpenPane(Box::new(identity)),
    }
}
