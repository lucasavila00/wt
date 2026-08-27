use super::super::refresh_status::RefreshStatus;
use super::layout::{
    card_grid, pane_card_grid, session_card_at_position, CARD_COLUMNS, WORLD_CARD_HEIGHT,
};
use super::{
    command_palette_layout, control_areas, Activity, CommandPalette, ControlAction, Help, PaneCard,
    PaneCardIdentity,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use wt_control_protocol::ResourceCapacity;

#[derive(Debug)]
pub(in crate::shell) struct ControlState {
    pub(super) activity: Activity,
    palette: CommandPalette,
    help: Help,
    pub(super) panes: Vec<PaneCard>,
    pub(super) selected: Option<PaneCardIdentity>,
    pane_scroll: usize,
    world_scroll: usize,
    scrollbar_drag: Option<Activity>,
    worlds_refresh: RefreshStatus,
    pane_refresh: RefreshStatus,
    capacity: ResourceCapacity,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            activity: Activity::Panes,
            palette: CommandPalette::default(),
            help: Help::default(),
            panes: Vec::new(),
            selected: None,
            pane_scroll: 0,
            world_scroll: 0,
            scrollbar_drag: None,
            worlds_refresh: RefreshStatus::default(),
            pane_refresh: RefreshStatus::default(),
            capacity: Default::default(),
        }
    }
}

impl ControlState {
    pub(in crate::shell) fn capacity(&self) -> ResourceCapacity {
        self.capacity
    }

    pub(in crate::shell) fn set_capacity(&mut self, capacity: ResourceCapacity) {
        self.capacity = capacity;
    }

    pub(in crate::shell) fn show_worlds(&mut self) {
        self.activity = Activity::Worlds;
        self.palette.close();
        self.help.close();
    }

    pub(in crate::shell) fn activity(&self) -> Activity {
        self.activity
    }

    pub(in crate::shell) fn palette(&self) -> &CommandPalette {
        &self.palette
    }

    pub(in crate::shell) fn help(&self) -> &Help {
        &self.help
    }

    pub(in crate::shell) fn panes(&self) -> &[PaneCard] {
        &self.panes
    }

    pub(in crate::shell) fn selected(&self) -> Option<&PaneCardIdentity> {
        self.selected.as_ref()
    }

    pub(in crate::shell) fn worlds_refresh(&self) -> &RefreshStatus {
        &self.worlds_refresh
    }

    pub(in crate::shell) fn pane_refresh(&self) -> &RefreshStatus {
        &self.pane_refresh
    }

    pub(in crate::shell) fn pane_refresh_mut(&mut self) -> &mut RefreshStatus {
        &mut self.pane_refresh
    }

    pub(in crate::shell) fn finish_worlds_refresh(&mut self, result: Result<String, Vec<String>>) {
        self.worlds_refresh.finish(result);
    }

    pub(in crate::shell) fn world_scroll(&self) -> usize {
        self.world_scroll
    }

    pub(in crate::shell) fn pane_scroll(&self) -> usize {
        self.pane_scroll
    }

    pub(in crate::shell) fn set_panes(
        &mut self,
        panes: Vec<PaneCard>,
        updated_at: String,
        area: Rect,
    ) -> bool {
        let selected = self
            .selected
            .as_ref()
            .filter(|selected| panes.iter().any(|card| &card.identity == *selected))
            .cloned()
            .or_else(|| panes.first().map(|card| card.identity.clone()));
        self.panes = panes;
        self.selected = selected;
        self.select_first_visible_pane();
        self.keep_pane_selection_visible(area);
        self.pane_refresh.finish(Ok(updated_at));
        true
    }

    pub(in crate::shell) fn apply_pane_refresh(
        &mut self,
        panes: Vec<PaneCard>,
        failures: Vec<String>,
        updated_at: String,
        area: Rect,
    ) -> bool {
        self.pane_refresh_mut().set_failures(failures);
        if self.pane_refresh().failures().is_some() {
            return true;
        }
        self.set_panes(panes, updated_at, area)
    }

    pub(in crate::shell) fn resize(&mut self, area: Rect) {
        self.keep_pane_selection_visible(area);
    }

    pub(in crate::shell) fn scroll_worlds(&mut self, delta: isize, area: Rect, count: usize) {
        let maximum = card_grid(area, self.world_scroll, count, WORLD_CARD_HEIGHT).maximum_scroll();
        self.world_scroll = self.world_scroll.saturating_add_signed(delta).min(maximum);
    }

    pub(in crate::shell) fn keep_world_selection_visible(
        &mut self,
        area: Rect,
        selected: usize,
        count: usize,
    ) {
        self.world_scroll = reveal_card(
            self.world_scroll,
            selected,
            card_grid(area, self.world_scroll, count, WORLD_CARD_HEIGHT),
            WORLD_CARD_HEIGHT,
            super::layout::CARD_GAP,
        );
    }

    pub(in crate::shell) fn handle_world_scrollbar(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        count: usize,
    ) -> bool {
        if self.palette.is_open() || self.help.is_open() {
            return false;
        }
        let maximum = card_grid(area, self.world_scroll, count, WORLD_CARD_HEIGHT).maximum_scroll();
        self.handle_scrollbar(mouse, area, Activity::Worlds, maximum)
    }

    pub(in crate::shell) fn handle_key(
        &mut self,
        key: KeyEvent,
        area: Rect,
    ) -> Option<ControlAction> {
        if self.help.is_open() {
            if key.modifiers == KeyModifiers::NONE
                && matches!(key.code, KeyCode::Char('2') | KeyCode::F(2) | KeyCode::Esc)
            {
                self.help.close();
            }
            return None;
        }
        if self.palette.is_open() {
            return self.palette.handle_key(key).map(ControlAction::Command);
        }
        if key.modifiers != KeyModifiers::NONE {
            return None;
        }
        match key.code {
            KeyCode::Tab => {
                self.activity = self.activity.next();
                self.select_first_visible_pane();
                self.keep_pane_selection_visible(area);
            }
            KeyCode::Char('1') | KeyCode::F(1) => self.palette.open(),
            KeyCode::Char('2') | KeyCode::F(2) => self.help.toggle(),
            KeyCode::Up if self.activity != Activity::Worlds => {
                self.move_pane(-(CARD_COLUMNS as isize), area)
            }
            KeyCode::Down if self.activity != Activity::Worlds => {
                self.move_pane(CARD_COLUMNS as isize, area)
            }
            KeyCode::Left if self.activity != Activity::Worlds => self.move_pane(-1, area),
            KeyCode::Right if self.activity != Activity::Worlds => self.move_pane(1, area),
            _ => {}
        }
        None
    }

    pub(in crate::shell) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
    ) -> (bool, Option<ControlAction>) {
        if self.help.is_open() {
            return (true, None);
        }
        if self.palette.is_open() && mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return (true, None);
        }
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let (_, footer) = super::control_content_areas(area);
            if super::help_control_area(footer).contains((mouse.column, mouse.row).into()) {
                self.help.toggle();
                return (true, None);
            }
        }
        if self.activity != Activity::Worlds && !self.palette.is_open() {
            let maximum = pane_card_grid(
                area,
                self.activity,
                self.pane_scroll,
                self.visible_pane_len(),
            )
            .maximum_scroll();
            if self.handle_scrollbar(mouse, area, self.activity, maximum) {
                return (true, None);
            }
        }
        match mouse.kind {
            MouseEventKind::ScrollUp if self.activity != Activity::Worlds => {
                self.scroll_pane(-1, area);
                return (true, None);
            }
            MouseEventKind::ScrollDown if self.activity != Activity::Worlds => {
                self.scroll_pane(1, area);
                return (true, None);
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return (false, None),
        }
        if self.palette.is_open() {
            let (_, results) = command_palette_layout(control_areas(area).1);
            if results.contains((mouse.column, mouse.row).into()) {
                let index = usize::from(mouse.row.saturating_sub(results.y));
                if index < self.palette.matches().len() {
                    return (
                        true,
                        self.palette.execute(index).map(ControlAction::Command),
                    );
                }
            }
            return (true, None);
        }
        if let Some(activity) = super::super::activity::at_position(area, mouse.column, mouse.row) {
            self.activity = activity;
            self.select_first_visible_pane();
            self.keep_pane_selection_visible(area);
            return (true, None);
        }
        if self.activity != Activity::Worlds {
            if let Some(index) = session_card_at_position(
                area,
                self.activity,
                self.pane_scroll,
                self.visible_pane_len(),
                mouse.column,
                mouse.row,
            ) {
                self.selected = Some(self.visible_pane_identities()[index].clone());
                return (true, None);
            }
        }
        (false, None)
    }

    pub(in crate::shell) fn close(&mut self) {
        self.palette.close();
        self.help.close();
    }

    fn move_pane(&mut self, delta: isize, area: Rect) {
        let identities = self.visible_pane_identities();
        if identities.is_empty() {
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|selected| identities.iter().position(|identity| identity == selected))
            .unwrap_or_default();
        let selected = current
            .saturating_add_signed(delta)
            .min(identities.len().saturating_sub(1));
        self.selected = Some(identities[selected].clone());
        self.keep_pane_selection_visible(area);
    }

    fn keep_pane_selection_visible(&mut self, area: Rect) {
        let identities = self.visible_pane_identities();
        let grid = pane_card_grid(area, self.activity, self.pane_scroll, identities.len());
        self.pane_scroll = self.pane_scroll.min(grid.maximum_scroll());
        let Some(selected) = self
            .selected
            .as_ref()
            .and_then(|selected| identities.iter().position(|identity| identity == selected))
        else {
            self.pane_scroll = 0;
            return;
        };
        let (height, gap) = (super::layout::PANE_CARD_HEIGHT, super::layout::CARD_GAP);
        self.pane_scroll = reveal_card(self.pane_scroll, selected, grid, height, gap);
    }

    fn scroll_pane(&mut self, delta: isize, area: Rect) {
        let count = self.visible_pane_len();
        let maximum = pane_card_grid(area, self.activity, self.pane_scroll, count).maximum_scroll();
        self.pane_scroll = self.pane_scroll.saturating_add_signed(delta).min(maximum);
    }

    fn handle_scrollbar(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        activity: Activity,
        maximum: usize,
    ) -> bool {
        if mouse.kind == MouseEventKind::Up(MouseButton::Left)
            && self.scrollbar_drag == Some(activity)
        {
            self.scrollbar_drag = None;
            return true;
        }
        let dragging = mouse.kind == MouseEventKind::Drag(MouseButton::Left)
            && self.scrollbar_drag == Some(activity);
        let pressed = mouse.kind == MouseEventKind::Down(MouseButton::Left);
        if !dragging && !pressed {
            return false;
        }
        let position = if dragging {
            super::super::scrollbar::drag_position(area, mouse.row, maximum)
        } else {
            let Some(position) =
                super::super::scrollbar::position_at(area, mouse.column, mouse.row, maximum)
            else {
                return false;
            };
            position
        };
        if pressed {
            self.scrollbar_drag = Some(activity);
        }
        if activity == Activity::Worlds {
            self.world_scroll = position;
        } else {
            self.pane_scroll = position;
        }
        true
    }
}

fn reveal_card(
    scroll: usize,
    index: usize,
    grid: super::layout::CardGrid,
    height: u16,
    gap: u16,
) -> usize {
    let top = index / CARD_COLUMNS * usize::from(height + gap);
    let bottom = top.saturating_add(usize::from(height));
    if top < scroll {
        top
    } else if bottom > scroll.saturating_add(usize::from(grid.viewport.height)) {
        bottom
            .saturating_sub(usize::from(grid.viewport.height))
            .min(grid.maximum_scroll())
    } else {
        scroll.min(grid.maximum_scroll())
    }
}
