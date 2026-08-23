use super::super::refresh_status::RefreshStatus;
use super::layout::{codex_visible_cards, session_card_at_position};
use super::{
    command_palette_layout, control_areas, Activity, CodexCard, CodexCardIdentity, CodexOpenTarget,
    CommandPalette, ControlAction,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use wt_control_protocol::ResourceCapacity;

#[derive(Debug)]
pub(in crate::shell) struct ControlState {
    pub(super) activity: Activity,
    palette: CommandPalette,
    pub(super) codex: Vec<CodexCard>,
    pub(super) selected: Option<CodexCardIdentity>,
    codex_offset: usize,
    pub(in crate::shell) opening: Option<CodexCardIdentity>,
    pub(in crate::shell) open_failure: Option<CodexOpenTarget>,
    worlds_refresh: RefreshStatus,
    codex_refresh: RefreshStatus,
    capacity: ResourceCapacity,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            activity: Activity::Codex,
            palette: CommandPalette::default(),
            codex: Vec::new(),
            selected: None,
            codex_offset: 0,
            opening: None,
            open_failure: None,
            worlds_refresh: RefreshStatus::default(),
            codex_refresh: RefreshStatus::default(),
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
    }

    pub(in crate::shell) fn activity(&self) -> Activity {
        self.activity
    }

    pub(in crate::shell) fn palette(&self) -> &CommandPalette {
        &self.palette
    }

    pub(in crate::shell) fn codex(&self) -> &[CodexCard] {
        &self.codex
    }

    pub(in crate::shell) fn selected(&self) -> Option<&CodexCardIdentity> {
        self.selected.as_ref()
    }

    pub(in crate::shell) fn worlds_refresh(&self) -> &RefreshStatus {
        &self.worlds_refresh
    }

    pub(in crate::shell) fn codex_refresh(&self) -> &RefreshStatus {
        &self.codex_refresh
    }

    pub(in crate::shell) fn codex_refresh_mut(&mut self) -> &mut RefreshStatus {
        &mut self.codex_refresh
    }

    pub(in crate::shell) fn finish_worlds_refresh(&mut self, result: Result<String, Vec<String>>) {
        self.worlds_refresh.finish(result);
    }

    pub(in crate::shell) fn codex_offset(&self) -> usize {
        self.codex_offset
    }

    pub(in crate::shell) fn opening(&self) -> Option<&CodexCardIdentity> {
        self.opening.as_ref()
    }

    pub(in crate::shell) fn set_codex(
        &mut self,
        codex: Vec<CodexCard>,
        updated_at: String,
        area: Rect,
    ) -> bool {
        if self.opening.is_some() {
            return false;
        }
        let selected = self
            .selected
            .as_ref()
            .filter(|selected| codex.iter().any(|card| &card.identity == *selected))
            .cloned()
            .or_else(|| codex.first().map(|card| card.identity.clone()));
        self.codex = codex;
        self.selected = selected;
        self.select_first_visible_codex();
        self.keep_codex_selection_visible(area);
        self.codex_refresh.finish(Ok(updated_at));
        true
    }

    pub(in crate::shell) fn apply_codex_refresh(
        &mut self,
        codex: Vec<CodexCard>,
        failures: Vec<String>,
        updated_at: String,
        area: Rect,
    ) -> bool {
        self.set_context_failures(failures);
        if self.context_failure().is_some() {
            return true;
        }
        self.set_codex(codex, updated_at, area)
    }

    pub(in crate::shell) fn resize(&mut self, area: Rect) {
        self.keep_codex_selection_visible(area);
    }

    pub(in crate::shell) fn handle_key(
        &mut self,
        key: KeyEvent,
        area: Rect,
    ) -> Option<ControlAction> {
        if self.open_failure.is_some() && key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Enter => return self.retry_open(),
                KeyCode::Esc => {
                    self.open_failure = None;
                    return None;
                }
                _ => {}
            }
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
                self.select_first_visible_codex();
                self.keep_codex_selection_visible(area);
            }
            KeyCode::Char('1') | KeyCode::F(1) => self.palette.open(),
            KeyCode::Up if self.activity != Activity::Worlds => {
                self.move_codex(-(super::super::live::columns(area) as isize), area)
            }
            KeyCode::Down if self.activity != Activity::Worlds => {
                self.move_codex(super::super::live::columns(area) as isize, area)
            }
            KeyCode::Left if self.activity == Activity::Live => self.move_codex(-1, area),
            KeyCode::Right if self.activity == Activity::Live => self.move_codex(1, area),
            KeyCode::Enter if self.activity != Activity::Worlds => {
                return self
                    .activate_selected()
                    .map(Box::new)
                    .map(ControlAction::OpenCodex)
            }
            _ => {}
        }
        None
    }

    pub(in crate::shell) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
    ) -> (bool, Option<ControlAction>) {
        if self.palette.is_open() && mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return (true, None);
        }
        match mouse.kind {
            MouseEventKind::ScrollUp if self.activity != Activity::Worlds => {
                self.move_codex(-(super::super::live::columns(area) as isize), area);
                return (true, None);
            }
            MouseEventKind::ScrollDown if self.activity != Activity::Worlds => {
                self.move_codex(super::super::live::columns(area) as isize, area);
                return (true, None);
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return (false, None),
        }
        if self.open_failure.is_some() {
            let (retry, dismiss) = super::super::toast::actions(area);
            let position = (mouse.column, mouse.row).into();
            if retry.contains(position) {
                return (true, self.retry_open());
            }
            if dismiss.contains(position) {
                self.open_failure = None;
                return (true, None);
            }
            if super::super::toast::area(area).contains(position) {
                return (true, None);
            }
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
            self.select_first_visible_codex();
            self.keep_codex_selection_visible(area);
            return (true, None);
        }
        if self.activity != Activity::Worlds {
            if self.opening.is_some() {
                return (true, None);
            }
            if let Some(index) = session_card_at_position(
                area,
                self.activity,
                self.codex_offset,
                self.visible_codex_len(),
                mouse.column,
                mouse.row,
            ) {
                self.selected = Some(self.visible_codex_identities()[index].clone());
                return (
                    true,
                    self.activate_selected()
                        .map(Box::new)
                        .map(ControlAction::OpenCodex),
                );
            }
        }
        (false, None)
    }

    pub(in crate::shell) fn close(&mut self) {
        self.palette.close();
    }

    fn activate_selected(&mut self) -> Option<CodexOpenTarget> {
        if self.opening.is_some() {
            return None;
        }
        let selected = self.selected.as_ref()?;
        let target = self
            .codex
            .iter()
            .find(|card| &card.identity == selected)?
            .open_target()?;
        self.opening = Some(target.identity.clone());
        self.open_failure = None;
        Some(target)
    }

    fn move_codex(&mut self, delta: isize, area: Rect) {
        let identities = self.visible_codex_identities();
        if identities.is_empty() || self.opening.is_some() {
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
        let visible = codex_visible_cards(area, self.activity).max(1);
        if self.activity == Activity::Live {
            let columns = super::super::live::columns(area);
            if selected < self.codex_offset {
                self.codex_offset = selected / columns * columns;
            } else if selected >= self.codex_offset.saturating_add(visible) {
                self.codex_offset = (selected / columns + 1 - visible / columns) * columns;
            }
            return;
        }
        if selected < self.codex_offset {
            self.codex_offset = selected;
        } else if selected >= self.codex_offset.saturating_add(visible) {
            self.codex_offset = selected + 1 - visible;
        }
    }

    fn keep_codex_selection_visible(&mut self, area: Rect) {
        let identities = self.visible_codex_identities();
        let visible = codex_visible_cards(area, self.activity).max(1);
        self.codex_offset = self
            .codex_offset
            .min(identities.len().saturating_sub(visible));
        let Some(selected) = self
            .selected
            .as_ref()
            .and_then(|selected| identities.iter().position(|identity| identity == selected))
        else {
            self.codex_offset = 0;
            return;
        };
        if self.activity == Activity::Live {
            let columns = super::super::live::columns(area);
            self.codex_offset -= self.codex_offset % columns;
            if selected < self.codex_offset {
                self.codex_offset = selected / columns * columns;
            } else if selected >= self.codex_offset.saturating_add(visible) {
                self.codex_offset = (selected / columns + 1 - visible / columns) * columns;
            }
            return;
        }
        if selected < self.codex_offset {
            self.codex_offset = selected;
        } else if selected >= self.codex_offset.saturating_add(visible) {
            self.codex_offset = selected + 1 - visible;
        }
    }
}
