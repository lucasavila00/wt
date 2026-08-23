use super::model::{ShellWorld, WorldIdentity};
use anyhow::{bail, Result};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use wt_client::config::ClientConfig;
use wt_control_protocol::{ApiRequest, Operation, Response};

const SEARCH_PICKER_MAX_HEIGHT: u16 = 18;
const CONFIRMATION_MAX_HEIGHT: u16 = 11;

pub(super) enum FlowAction {
    None,
    Changed,
    Submit(Box<ShellWorld>),
    Cancel,
}

pub(super) struct Flow {
    phase: Phase,
}

enum Phase {
    Pick(Picker),
    Confirm {
        world: ShellWorld,
        choice: ConfirmChoice,
    },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ConfirmChoice {
    Cancel,
    Delete,
}

impl Flow {
    pub(super) fn new(worlds: Vec<ShellWorld>) -> Self {
        Self {
            phase: Phase::Pick(Picker::new(worlds)),
        }
    }

    pub(super) fn confirm(world: ShellWorld) -> Self {
        Self {
            phase: Phase::Confirm {
                world,
                choice: ConfirmChoice::Cancel,
            },
        }
    }

    pub(super) fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        _config: &ClientConfig,
    ) -> FlowAction {
        match &mut self.phase {
            Phase::Pick(picker) => match picker.handle_event(event, area) {
                PickerEvent::Changed => FlowAction::Changed,
                PickerEvent::Cancel => FlowAction::Cancel,
                PickerEvent::Select(world) => {
                    self.phase = Phase::Confirm {
                        world,
                        choice: ConfirmChoice::Cancel,
                    };
                    FlowAction::Changed
                }
            },
            Phase::Confirm { world, choice } => {
                let event = confirmation_event(event, area, choice);
                match event {
                    ConfirmationEvent::Changed => FlowAction::Changed,
                    ConfirmationEvent::Cancel => FlowAction::Cancel,
                    ConfirmationEvent::Delete => FlowAction::Submit(Box::new(world.clone())),
                }
            }
        }
    }

    pub(super) fn handle_paste(&mut self, text: &str) -> FlowAction {
        let Phase::Pick(picker) = &mut self.phase else {
            return FlowAction::None;
        };
        picker
            .query
            .extend(text.chars().filter(|character| !character.is_control()));
        picker.rank();
        FlowAction::Changed
    }

    pub(super) fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        match &self.phase {
            Phase::Pick(picker) => picker.render(frame, area),
            Phase::Confirm { world, choice } => render_confirmation(frame, area, world, *choice),
        }
    }
}

pub(super) struct Task {
    world: ShellWorld,
    result: Receiver<Result<()>>,
}

impl Task {
    pub(super) fn start(config: &ClientConfig, world: ShellWorld) -> Result<Self> {
        let result = start_worker(config, &world)?;
        Ok(Self { world, result })
    }

    pub(super) fn poll(&self) -> Option<Result<WorldIdentity, String>> {
        match self.result.try_recv() {
            Ok(Ok(())) => Some(Ok(self.world.identity.clone())),
            Ok(Err(error)) => Some(Err(format!("{error:#}"))),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("world deletion worker stopped unexpectedly".into()))
            }
        }
    }
}

fn start_worker(config: &ClientConfig, world: &ShellWorld) -> Result<Receiver<Result<()>>> {
    let context = config
        .context(&world.identity.context)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!("context {} is no longer configured", world.identity.context)
        })?;
    let world_id = world.identity.world_id;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(format!("wt-shell-delete-{}", world.name))
        .spawn(move || {
            let result = wt_client::transport::call(
                &context,
                &ApiRequest::new(Operation::DeleteWorld { world_id }),
            )
            .map_err(anyhow::Error::from)
            .and_then(|response| match response {
                Response::WorldDeleted { .. } => Ok(()),
                _ => bail!("helper returned the wrong response to delete"),
            });
            let _ = sender.send(result);
        })?;
    Ok(receiver)
}

struct Picker {
    worlds: Vec<ShellWorld>,
    query: String,
    matches: Vec<usize>,
    selected: Option<usize>,
    offset: usize,
}

enum PickerEvent {
    Changed,
    Cancel,
    Select(ShellWorld),
}

impl Picker {
    fn new(worlds: Vec<ShellWorld>) -> Self {
        let mut picker = Self {
            worlds,
            query: String::new(),
            matches: Vec::new(),
            selected: None,
            offset: 0,
        };
        picker.rank();
        picker
    }

    fn rank(&mut self) {
        let query = FuzzyQuery::new(&self.query);
        let mut matches = self
            .worlds
            .iter()
            .enumerate()
            .filter_map(|(index, world)| query.score(&world.name).map(|score| (score, index)))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        self.matches = matches.into_iter().map(|(_, index)| index).collect();
        self.selected = (!self.matches.is_empty()).then_some(0);
        self.offset = 0;
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> PickerEvent {
        let (_, results) = picker_layout(area);
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Esc => PickerEvent::Cancel,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.rank();
                    PickerEvent::Changed
                }
                KeyCode::Up => {
                    self.move_selection(-1, usize::from(results.height));
                    PickerEvent::Changed
                }
                KeyCode::Down => {
                    self.move_selection(1, usize::from(results.height));
                    PickerEvent::Changed
                }
                KeyCode::Enter => self
                    .selected
                    .and_then(|selected| self.matches.get(selected))
                    .map(|index| PickerEvent::Select(self.worlds[*index].clone()))
                    .unwrap_or(PickerEvent::Changed),
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.query.push(character);
                    self.rank();
                    PickerEvent::Changed
                }
                _ => PickerEvent::Changed,
            },
            Event::Mouse(mouse) if results.contains((mouse.column, mouse.row).into()) => {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        self.offset = self.offset.saturating_sub(1);
                        PickerEvent::Changed
                    }
                    MouseEventKind::ScrollDown => {
                        self.offset = self.offset.saturating_add(1).min(
                            self.matches
                                .len()
                                .saturating_sub(usize::from(results.height)),
                        );
                        PickerEvent::Changed
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        let selected = self
                            .offset
                            .saturating_add(usize::from(mouse.row.saturating_sub(results.y)));
                        self.matches
                            .get(selected)
                            .map(|index| PickerEvent::Select(self.worlds[*index].clone()))
                            .unwrap_or(PickerEvent::Changed)
                    }
                    _ => PickerEvent::Changed,
                }
            }
            _ => PickerEvent::Changed,
        }
    }

    fn move_selection(&mut self, amount: i64, viewport: usize) {
        let Some(selected) = self.selected else {
            return;
        };
        let maximum = self.matches.len().saturating_sub(1);
        let next = if amount < 0 {
            selected.saturating_sub(1)
        } else {
            selected.saturating_add(1).min(maximum)
        };
        self.selected = Some(next);
        if next < self.offset {
            self.offset = next;
        } else if next >= self.offset.saturating_add(viewport) {
            self.offset = next.saturating_add(1).saturating_sub(viewport);
        }
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let (modal, results) = picker_layout(area);
        frame.render_widget(Clear, modal);
        frame.render_widget(modal_block("Delete world"), modal);
        let sections = picker_sections(modal.inner(Margin::new(2, 1)));
        frame.render_widget(Paragraph::new(format!("> {}█", self.query)), sections[0]);
        frame.render_widget(
            Paragraph::new("─".repeat(usize::from(sections[1].width))).style(muted_style()),
            sections[1],
        );
        let items = if self.matches.is_empty() {
            vec![ListItem::new(if self.worlds.is_empty() {
                "No worlds to delete"
            } else {
                "No matching worlds"
            })
            .style(muted_style())]
        } else {
            self.matches
                .iter()
                .skip(self.offset)
                .take(usize::from(results.height))
                .map(|index| ListItem::new(self.worlds[*index].name.clone()))
                .collect()
        };
        let visible_selection = self
            .selected
            .and_then(|selected| selected.checked_sub(self.offset))
            .filter(|selected| *selected < usize::from(results.height));
        let list = List::new(items)
            .highlight_symbol(" ")
            .highlight_style(selected_style());
        let mut state = ListState::default().with_selected(visible_selection);
        frame.render_stateful_widget(list, results, &mut state);
        frame.render_widget(
            Paragraph::new("↑/↓ select · Enter choose · Esc close").style(muted_style()),
            sections[3],
        );
    }
}

enum ConfirmationEvent {
    Changed,
    Cancel,
    Delete,
}

fn confirmation_event(event: &Event, area: Rect, choice: &mut ConfirmChoice) -> ConfirmationEvent {
    match event {
        Event::Key(key) => match key.code {
            KeyCode::Esc => ConfirmationEvent::Cancel,
            KeyCode::Enter => match choice {
                ConfirmChoice::Cancel => ConfirmationEvent::Cancel,
                ConfirmChoice::Delete => ConfirmationEvent::Delete,
            },
            KeyCode::Left | KeyCode::Up => {
                *choice = ConfirmChoice::Cancel;
                ConfirmationEvent::Changed
            }
            KeyCode::Right | KeyCode::Down => {
                *choice = ConfirmChoice::Delete;
                ConfirmationEvent::Changed
            }
            _ => ConfirmationEvent::Changed,
        },
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            let layout = confirmation_layout(area);
            let position = (mouse.column, mouse.row).into();
            if layout.cancel.contains(position) {
                ConfirmationEvent::Cancel
            } else if layout.delete.contains(position) {
                ConfirmationEvent::Delete
            } else {
                ConfirmationEvent::Changed
            }
        }
        _ => ConfirmationEvent::Changed,
    }
}

fn render_confirmation(
    frame: &mut Frame<'_>,
    area: Rect,
    world: &ShellWorld,
    choice: ConfirmChoice,
) {
    let layout = confirmation_layout(area);
    frame.render_widget(Clear, layout.modal);
    frame.render_widget(modal_block("Delete world?"), layout.modal);
    frame.render_widget(
        Paragraph::new(format!(
            "Delete world \"{}\"?\n\nThis cannot be undone.",
            world.name
        )),
        layout.message,
    );
    frame.render_widget(
        Paragraph::new("[ Cancel ]")
            .alignment(Alignment::Center)
            .style(button_style(choice == ConfirmChoice::Cancel)),
        layout.cancel,
    );
    frame.render_widget(
        Paragraph::new("[ Delete world ]")
            .alignment(Alignment::Center)
            .style(button_style(choice == ConfirmChoice::Delete)),
        layout.delete,
    );
    frame.render_widget(
        Paragraph::new("Arrows: select · Enter: choose · Esc: cancel")
            .alignment(Alignment::Center)
            .style(muted_style()),
        layout.footer,
    );
}

fn button_style(selected: bool) -> Style {
    if selected {
        selected_style()
    } else {
        Style::new()
    }
}

fn modal_block(title: &str) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
}

fn muted_style() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

fn selected_style() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

fn picker_layout(area: Rect) -> (Rect, Rect) {
    let width = responsive_width(area.width);
    let top = area.y.saturating_add(area.height.saturating_mul(20) / 100);
    let height = SEARCH_PICKER_MAX_HEIGHT.min(area.bottom().saturating_sub(top));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    );
    let sections = picker_sections(modal.inner(Margin::new(2, 1)));
    (modal, sections[2])
}

fn picker_sections(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area)
}

struct ConfirmationLayout {
    modal: Rect,
    message: Rect,
    cancel: Rect,
    delete: Rect,
    footer: Rect,
}

fn confirmation_layout(area: Rect) -> ConfirmationLayout {
    let width = responsive_width(area.width);
    let height = CONFIRMATION_MAX_HEIGHT.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(modal.inner(Margin::new(2, 1)));
    let buttons =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    ConfirmationLayout {
        modal,
        message: rows[0],
        cancel: buttons[0],
        delete: buttons[1],
        footer: rows[3],
    }
}

fn responsive_width(available: u16) -> u16 {
    (available.saturating_mul(70) / 100).clamp(30.min(available), 80.min(available))
}

struct FuzzyQuery(Atom);

impl FuzzyQuery {
    fn new(query: &str) -> Self {
        Self(Atom::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
            false,
        ))
    }

    fn score(&self, candidate: &str) -> Option<u16> {
        thread_local! {
            static MATCHER: std::cell::RefCell<(Matcher, Vec<char>)> =
                std::cell::RefCell::new((Matcher::new(Config::DEFAULT), Vec::new()));
        }
        MATCHER.with_borrow_mut(|state| {
            let (matcher, buffer) = state;
            self.0.score(Utf32Str::new(candidate, buffer), matcher)
        })
    }
}

#[cfg(test)]
#[path = "delete_tests.rs"]
mod tests;
