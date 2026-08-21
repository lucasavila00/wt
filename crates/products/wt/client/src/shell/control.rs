use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState};

pub(super) const COMMANDS: [ControlCommand; 2] =
    [ControlCommand::NewWorld, ControlCommand::DeleteWorld];
pub(super) const ACTIVITY_BAR_WIDTH: u16 = 5;
pub(super) const ACTIVITY_BUTTON_HEIGHT: u16 = 3;
pub(super) const CODEX_CARD_HEIGHT: u16 = 5;

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
    pub(super) kind: CodexCardKind,
}

impl CodexCard {
    pub(super) fn rollout_only(context: &str, session_id: Uuid, timestamp: i64) -> Self {
        Self {
            identity: CodexCardIdentity::RolloutOnly {
                context: context.into(),
                session_id,
            },
            context: context.into(),
            session_id: Some(session_id),
            timestamp: Some(timestamp),
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
pub(super) enum Activity {
    Worlds,
    Codex,
}

impl Activity {
    fn next(self) -> Self {
        match self {
            Self::Worlds => Self::Codex,
            Self::Codex => Self::Worlds,
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

impl ControlCommand {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::NewWorld => "World: New",
            Self::DeleteWorld => "World: Delete...",
        }
    }
}

#[derive(Debug)]
pub(super) struct ControlState {
    activity: Activity,
    palette: CommandPalette,
    codex: Vec<CodexCard>,
    selected: Option<CodexCardIdentity>,
    codex_offset: usize,
    opening: Option<CodexCardIdentity>,
    open_error: Option<(CodexCardIdentity, String)>,
    worlds_updated_at: Option<String>,
    codex_updated_at: Option<String>,
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
            open_error: None,
            worlds_updated_at: None,
            codex_updated_at: None,
        }
    }
}

impl ControlState {
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

    pub(super) fn worlds_updated_at(&self) -> Option<&str> {
        self.worlds_updated_at.as_deref()
    }

    pub(super) fn codex_updated_at(&self) -> Option<&str> {
        self.codex_updated_at.as_deref()
    }

    pub(super) fn set_worlds_updated_at(&mut self, updated_at: String) {
        self.worlds_updated_at = Some(updated_at);
    }

    pub(super) fn codex_offset(&self) -> usize {
        self.codex_offset
    }

    pub(super) fn opening(&self) -> Option<&CodexCardIdentity> {
        self.opening.as_ref()
    }

    pub(super) fn open_error(&self, identity: &CodexCardIdentity) -> Option<&str> {
        self.open_error
            .as_ref()
            .filter(|(target, _)| target == identity)
            .map(|(_, message)| message.as_str())
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
        self.keep_codex_selection_visible(area);
        self.codex_updated_at = Some(updated_at);
        true
    }

    pub(super) fn resize(&mut self, area: Rect) {
        self.keep_codex_selection_visible(area);
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, area: Rect) -> Option<ControlAction> {
        if self.palette.is_open() {
            return self.palette.handle_key(key).map(ControlAction::Command);
        }
        if key.modifiers != KeyModifiers::NONE {
            return None;
        }
        match key.code {
            KeyCode::Tab => self.activity = self.activity.next(),
            KeyCode::Char('1') | KeyCode::F(1) => self.palette.open(),
            KeyCode::Up if self.activity == Activity::Codex => self.move_codex(-1, area),
            KeyCode::Down if self.activity == Activity::Codex => self.move_codex(1, area),
            KeyCode::Enter if self.activity == Activity::Codex => {
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
            MouseEventKind::ScrollUp if self.activity == Activity::Codex => {
                self.move_codex(-3, area);
                return (true, None);
            }
            MouseEventKind::ScrollDown if self.activity == Activity::Codex => {
                self.move_codex(3, area);
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
        if let Some(activity) = activity_at_position(area, mouse.column, mouse.row) {
            self.activity = activity;
            return (true, None);
        }
        if self.activity == Activity::Codex {
            if self.opening.is_some() {
                return (true, None);
            }
            if let Some(index) = codex_card_at_position(
                area,
                self.codex_offset,
                self.codex.len(),
                mouse.column,
                mouse.row,
            ) {
                self.selected = Some(self.codex[index].identity.clone());
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

    pub(super) fn finish_open(
        &mut self,
        identity: &CodexCardIdentity,
        error: Option<String>,
    ) -> bool {
        if self.opening.as_ref() != Some(identity) {
            return false;
        }
        self.opening = None;
        self.open_error = error.map(|message| (identity.clone(), message));
        true
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
        self.open_error = None;
        Some(target)
    }

    fn move_codex(&mut self, delta: isize, area: Rect) {
        if self.codex.is_empty() || self.opening.is_some() {
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|selected| {
                self.codex
                    .iter()
                    .position(|card| &card.identity == selected)
            })
            .unwrap_or_default();
        let selected = current
            .saturating_add_signed(delta)
            .min(self.codex.len().saturating_sub(1));
        self.selected = Some(self.codex[selected].identity.clone());
        let visible = codex_visible_cards(area).max(1);
        if selected < self.codex_offset {
            self.codex_offset = selected;
        } else if selected >= self.codex_offset.saturating_add(visible) {
            self.codex_offset = selected + 1 - visible;
        }
    }

    fn keep_codex_selection_visible(&mut self, area: Rect) {
        let visible = codex_visible_cards(area).max(1);
        self.codex_offset = self
            .codex_offset
            .min(self.codex.len().saturating_sub(visible));
        let Some(selected) = self.selected.as_ref().and_then(|selected| {
            self.codex
                .iter()
                .position(|card| &card.identity == selected)
        }) else {
            self.codex_offset = 0;
            return;
        };
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
    let (body, _) = control_content_areas(area);
    let viewport = body.inner(Margin::new(1, 1));
    if viewport.is_empty() {
        return Vec::new();
    }
    let visible = usize::from(viewport.height.div_ceil(CODEX_CARD_HEIGHT));
    (offset..count.min(offset.saturating_add(visible)))
        .enumerate()
        .map(|(row, index)| {
            (
                index,
                Rect::new(
                    viewport.x,
                    viewport.y + u16::try_from(row).unwrap_or(u16::MAX) * CODEX_CARD_HEIGHT,
                    viewport.width,
                    CODEX_CARD_HEIGHT.min(
                        viewport
                            .bottom()
                            .saturating_sub(viewport.y + row as u16 * CODEX_CARD_HEIGHT),
                    ),
                ),
            )
        })
        .collect()
}

fn codex_visible_cards(area: Rect) -> usize {
    let (body, _) = control_content_areas(area);
    usize::from(
        body.inner(Margin::new(1, 1))
            .height
            .div_ceil(CODEX_CARD_HEIGHT),
    )
}

fn codex_card_at_position(
    area: Rect,
    offset: usize,
    count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    codex_card_rects(area, offset, count)
        .into_iter()
        .find(|(_, rect)| rect.contains((column, row).into()))
        .map(|(index, _)| index)
}

pub(super) fn command_palette_layout(content: Rect) -> (Rect, Rect) {
    let width = (content.width.saturating_mul(70) / 100)
        .clamp(30.min(content.width), 70.min(content.width));
    let height = 9.min(content.height);
    let area = Rect::new(
        content.x + content.width.saturating_sub(width) / 2,
        content.y + content.height.saturating_mul(20) / 100,
        width,
        height,
    );
    let inner = area.inner(Margin::new(1, 1));
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    (area, rows[2])
}

fn activity_at_position(area: Rect, column: u16, row: u16) -> Option<Activity> {
    let (bar, _) = control_areas(area);
    if column < bar.x
        || column >= bar.right().saturating_sub(1)
        || row < bar.y
        || row >= bar.bottom()
    {
        return None;
    }
    match row.saturating_sub(bar.y) / ACTIVITY_BUTTON_HEIGHT {
        0 => Some(Activity::Codex),
        1 => Some(Activity::Worlds),
        _ => None,
    }
}

#[cfg(test)]
#[path = "control/tests.rs"]
mod tests;
