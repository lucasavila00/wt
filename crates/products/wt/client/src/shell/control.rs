use super::refresh_status::RefreshStatus;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState};

mod live;

pub(super) use super::activity::Activity;

pub(super) const COMMANDS: [ControlCommand; 2] =
    [ControlCommand::NewWorld, ControlCommand::DeleteWorld];
pub(super) const ACTIVITY_BAR_WIDTH: u16 = 5;
pub(super) const ACTIVITY_BUTTON_HEIGHT: u16 = 3;
pub(super) const CODEX_CARD_HEIGHT: u16 = 8;
pub(super) const WORLD_CARD_HEIGHT: u16 = 10;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum CodexCardIdentity {
    Observation {
        context: String,
        session_id: Uuid,
        world_id: Uuid,
        tmux_session: String,
        pane_id: String,
    },
    RolloutOnly {
        context: String,
        session_id: Uuid,
    },
    ContextError {
        context: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CodexOpenTarget {
    pub(super) identity: CodexCardIdentity,
    pub(super) context: String,
    pub(super) session_id: Uuid,
    pub(super) world_id: Uuid,
    pub(super) tmux_session: String,
    pub(super) pane_id: String,
}

#[derive(Clone, Debug)]
pub(super) enum CodexCardKind {
    Observation {
        world_id: Uuid,
        world_name: String,
        cwd: String,
        repository_root: Option<String>,
        repository_url: Option<String>,
        git_branch: Option<String>,
        state: CodexSessionState,
        session_start_source: Option<String>,
        target: ByobuTarget,
    },
    RolloutOnly,
    ContextError {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(super) struct CodexCard {
    pub(super) identity: CodexCardIdentity,
    pub(super) context: String,
    pub(super) session_id: Option<Uuid>,
    pub(super) timestamp: Option<i64>,
    pub(super) latest_user_message: Option<String>,
    pub(super) kind: CodexCardKind,
}

impl CodexCard {
    pub(super) fn rollout_only(
        context: &str,
        session_id: Uuid,
        timestamp: i64,
        latest_user_message: Option<String>,
    ) -> Self {
        Self {
            identity: CodexCardIdentity::RolloutOnly {
                context: context.into(),
                session_id,
            },
            context: context.into(),
            session_id: Some(session_id),
            timestamp: Some(timestamp),
            latest_user_message,
            kind: CodexCardKind::RolloutOnly,
        }
    }

    pub(super) fn context_error(context: &str, message: String) -> Self {
        Self {
            identity: CodexCardIdentity::ContextError {
                context: context.into(),
            },
            context: context.into(),
            session_id: None,
            timestamp: None,
            latest_user_message: None,
            kind: CodexCardKind::ContextError { message },
        }
    }

    pub(super) fn open_target(&self) -> Option<CodexOpenTarget> {
        let CodexCardKind::Observation {
            world_id,
            state,
            target,
            ..
        } = &self.kind
        else {
            return None;
        };
        if *state == CodexSessionState::Inactive {
            return None;
        }
        Some(CodexOpenTarget {
            identity: self.identity.clone(),
            context: self.context.clone(),
            session_id: self.session_id.expect("observation card has session ID"),
            world_id: *world_id,
            tmux_session: target.tmux_session.clone(),
            pane_id: target.pane_id.clone(),
        })
    }

    pub(super) fn sort_rank(&self) -> u8 {
        match &self.kind {
            CodexCardKind::Observation { state, .. } => match state {
                CodexSessionState::NeedsAttention => 0,
                CodexSessionState::Working => 1,
                CodexSessionState::Unknown => 2,
                CodexSessionState::Inactive => 3,
            },
            CodexCardKind::RolloutOnly => 4,
            CodexCardKind::ContextError { .. } => 5,
        }
    }

    pub(super) fn timestamp(&self) -> i64 {
        self.timestamp.unwrap_or_default()
    }

    pub(super) fn disabled_reason(&self) -> Option<&'static str> {
        match &self.kind {
            CodexCardKind::Observation {
                state: CodexSessionState::Inactive,
                ..
            } => Some("session ended"),
            CodexCardKind::RolloutOnly => Some("session is not open in a WT pane"),
            CodexCardKind::ContextError { .. } => Some("context data rejected"),
            CodexCardKind::Observation { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ControlCommand {
    NewWorld,
    DeleteWorld,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ControlAction {
    Command(ControlCommand),
    OpenCodex(Box<CodexOpenTarget>),
}

#[derive(Debug)]
pub(super) struct ControlState {
    activity: Activity,
    palette: CommandPalette,
    codex: Vec<CodexCard>,
    selected: Option<CodexCardIdentity>,
    codex_offset: usize,
    pub(super) opening: Option<CodexCardIdentity>,
    pub(super) open_failure: Option<CodexOpenTarget>,
    worlds_refresh: RefreshStatus,
    codex_refresh: RefreshStatus,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            activity: Activity::Live,
            palette: CommandPalette::default(),
            codex: Vec::new(),
            selected: None,
            codex_offset: 0,
            opening: None,
            open_failure: None,
            worlds_refresh: RefreshStatus::default(),
            codex_refresh: RefreshStatus::default(),
        }
    }
}

impl ControlState {
    pub(super) fn show_worlds(&mut self) {
        self.activity = Activity::Worlds;
        self.palette.close();
    }
    pub(super) fn activity(&self) -> Activity {
        self.activity
    }

    pub(super) fn palette(&self) -> &CommandPalette {
        &self.palette
    }

    pub(super) fn codex(&self) -> &[CodexCard] {
        &self.codex
    }

    pub(super) fn selected(&self) -> Option<&CodexCardIdentity> {
        self.selected.as_ref()
    }

    pub(super) fn worlds_refresh(&self) -> &RefreshStatus {
        &self.worlds_refresh
    }

    pub(super) fn codex_refresh(&self) -> &RefreshStatus {
        &self.codex_refresh
    }

    pub(super) fn codex_refresh_mut(&mut self) -> &mut RefreshStatus {
        &mut self.codex_refresh
    }

    pub(super) fn finish_worlds_refresh(&mut self, result: Result<String, Vec<String>>) {
        self.worlds_refresh.finish(result);
    }

    pub(super) fn codex_offset(&self) -> usize {
        self.codex_offset
    }

    pub(super) fn opening(&self) -> Option<&CodexCardIdentity> {
        self.opening.as_ref()
    }

    pub(super) fn set_codex(
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

    pub(super) fn apply_codex_refresh(
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

    pub(super) fn resize(&mut self, area: Rect) {
        self.keep_codex_selection_visible(area);
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> Option<ControlAction> {
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
                self.move_codex(-(super::live::columns(area) as isize), area)
            }
            KeyCode::Down if self.activity != Activity::Worlds => {
                self.move_codex(super::live::columns(area) as isize, area)
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

    pub(super) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
    ) -> (bool, Option<ControlAction>) {
        if self.palette.is_open() && mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return (true, None);
        }
        match mouse.kind {
            MouseEventKind::ScrollUp if self.activity != Activity::Worlds => {
                self.move_codex(-(super::live::columns(area) as isize), area);
                return (true, None);
            }
            MouseEventKind::ScrollDown if self.activity != Activity::Worlds => {
                self.move_codex(super::live::columns(area) as isize, area);
                return (true, None);
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return (false, None),
        }
        if self.open_failure.is_some() {
            let (retry, dismiss) = super::toast::actions(area);
            let position = (mouse.column, mouse.row).into();
            if retry.contains(position) {
                return (true, self.retry_open());
            }
            if dismiss.contains(position) {
                self.open_failure = None;
                return (true, None);
            }
            if super::toast::area(area).contains(position) {
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
        if let Some(activity) = super::activity::at_position(area, mouse.column, mouse.row) {
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

    pub(super) fn close(&mut self) {
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
            let columns = super::live::columns(area);
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
            let columns = super::live::columns(area);
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

#[derive(Debug, Default)]
pub(super) struct CommandPalette {
    open: bool,
    query: String,
    selected: usize,
}

impl CommandPalette {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn matches(&self) -> Vec<ControlCommand> {
        let query = self.query.to_ascii_lowercase();
        COMMANDS
            .iter()
            .copied()
            .filter(|command| command.label().to_ascii_lowercase().contains(&query))
            .collect()
    }

    fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<ControlCommand> {
        match key.code {
            KeyCode::Esc => self.close(),
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.matches().len().saturating_sub(1));
            }
            KeyCode::Enter => {
                return self.execute(self.selected);
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.selected = 0;
            }
            _ => {}
        }
        None
    }

    fn execute(&mut self, index: usize) -> Option<ControlCommand> {
        let command = self.matches().get(index).copied();
        self.close();
        command
    }
}

pub(super) fn control_areas(area: Rect) -> (Rect, Rect) {
    let columns = Layout::horizontal([Constraint::Length(ACTIVITY_BAR_WIDTH), Constraint::Min(0)])
        .split(area);
    (columns[0], columns[1])
}

pub(super) fn control_content_areas(area: Rect) -> (Rect, Rect) {
    let (_, content) = control_areas(area);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(content);
    (rows[0], rows[1])
}

pub(super) fn codex_card_rects(area: Rect, offset: usize, count: usize) -> Vec<(usize, Rect)> {
    session_card_rects(area, offset, count, CODEX_CARD_HEIGHT)
}

fn session_card_rects(
    area: Rect,
    offset: usize,
    count: usize,
    card_height: u16,
) -> Vec<(usize, Rect)> {
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    if viewport.is_empty() {
        return Vec::new();
    }
    let visible = usize::from(viewport.height.div_ceil(card_height));
    (offset..count.min(offset.saturating_add(visible)))
        .enumerate()
        .map(|(row, index)| {
            (
                index,
                Rect::new(
                    viewport.x,
                    viewport.y + u16::try_from(row).unwrap_or(u16::MAX) * card_height,
                    viewport.width,
                    card_height.min(
                        viewport
                            .bottom()
                            .saturating_sub(viewport.y + row as u16 * card_height),
                    ),
                ),
            )
        })
        .collect()
}

pub(super) fn world_card_rects(area: Rect, selected: usize, count: usize) -> Vec<(usize, Rect)> {
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    if viewport.is_empty() {
        return Vec::new();
    }
    let visible = usize::from(viewport.height.div_ceil(WORLD_CARD_HEIGHT)).max(1);
    let offset = selected / visible * visible;
    (offset..count.min(offset.saturating_add(visible)))
        .enumerate()
        .map(|(row, index)| {
            let y = viewport.y + u16::try_from(row).unwrap_or(u16::MAX) * WORLD_CARD_HEIGHT;
            (
                index,
                Rect::new(
                    viewport.x,
                    y,
                    viewport.width,
                    WORLD_CARD_HEIGHT.min(viewport.bottom().saturating_sub(y)),
                ),
            )
        })
        .collect()
}

pub(super) fn world_card_at_position(
    area: Rect,
    selected: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    world_card_rects(area, selected, count)
        .into_iter()
        .find(|(_, rect)| rect.contains((column, row).into()))
        .map(|(index, _)| index)
}

fn codex_visible_cards(area: Rect, activity: Activity) -> usize {
    if activity == Activity::Live {
        return super::live::visible(area);
    }
    let (body, _) = control_content_areas(area);
    usize::from(
        body.inner(Margin::new(1, 1))
            .height
            .div_ceil(CODEX_CARD_HEIGHT),
    )
}

fn session_card_at_position(
    area: Rect,
    activity: Activity,
    offset: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    let rects = if activity == Activity::Live {
        super::live::card_rects(area, offset, count)
    } else {
        codex_card_rects(area, offset, count)
    };
    rects
        .into_iter()
        .find(|(_, rect)| rect.contains((column, row).into()))
        .map(|(index, _)| index)
}

#[cfg(test)]
#[path = "control/tests.rs"]
mod tests;

#[path = "control/command.rs"]
mod command;
pub(super) use command::command_palette_layout;
