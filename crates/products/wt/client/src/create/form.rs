use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use wt_client::config::ClientConfig;
use wt_control_protocol::ResourceCapacity;
use wt_control_protocol::WorldName;

use crate::git_author::GitAuthor;

const DEFAULT_VCPUS: u32 = 2;
const DEFAULT_MEMORY_MIB: u64 = 4096;
const DEFAULT_DISK_GIB: u64 = 32;
const LABEL_WIDTH: usize = 16;
const ADJECTIVES: [&str; 16] = [
    "amber", "brisk", "bright", "calm", "clever", "curious", "eager", "gentle", "happy", "lucky",
    "nimble", "quiet", "swift", "vivid", "warm", "wise",
];
const ANIMALS: [&str; 16] = [
    "badger", "bison", "corgi", "falcon", "fox", "gecko", "heron", "koala", "orca", "otter",
    "panda", "puffin", "raven", "turtle", "wolf", "wombat",
];
const HOST_FIELDS: [Field; 5] = [
    Field::Context,
    Field::Name,
    Field::Vcpus,
    Field::Memory,
    Field::Disk,
];
const OK_FOCUS: usize = HOST_FIELDS.len();

#[derive(Clone, Debug)]
pub(crate) struct Input {
    pub(crate) context: String,
    pub(crate) name: WorldName,
    pub(crate) vcpus: u32,
    pub(crate) memory_mib: u64,
    pub(crate) disk_gib: u64,
    pub(crate) git_user_name: String,
    pub(crate) git_user_email: String,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    None,
    Cancel,
    Submit(Input),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    Fields,
    Review,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Field {
    Context,
    Name,
    Vcpus,
    Memory,
    Disk,
}

#[derive(Clone, Debug)]
pub(crate) struct Form {
    contexts: Vec<String>,
    capacities: std::collections::BTreeMap<String, ResourceCapacity>,
    context: usize,
    name: String,
    name_is_suggestion: bool,
    vcpus: String,
    memory: String,
    disk: String,
    author: GitAuthor,
    focus: usize,
    stage: Stage,
    error: Option<String>,
}

impl Form {
    pub(crate) fn new(
        config: &ClientConfig,
        author: GitAuthor,
        used_names: &std::collections::BTreeSet<String>,
        capacities: std::collections::BTreeMap<String, ResourceCapacity>,
    ) -> anyhow::Result<Self> {
        if config.contexts.is_empty() {
            anyhow::bail!("no contexts are configured");
        }
        Ok(Self {
            contexts: config
                .contexts
                .iter()
                .map(|context| context.name.clone())
                .collect(),
            capacities,
            context: 0,
            name: suggested_name(used_names),
            name_is_suggestion: true,
            vcpus: String::new(),
            memory: String::new(),
            disk: String::new(),
            author,
            focus: OK_FOCUS,
            stage: Stage::Fields,
            error: None,
        })
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Action::Cancel;
        }
        if self.stage == Stage::Review {
            return match key.code {
                KeyCode::Enter => self
                    .input()
                    .map_or_else(|error| self.fail(error), Action::Submit),
                KeyCode::Char('b') if key.modifiers == KeyModifiers::NONE => {
                    self.stage = Stage::Fields;
                    Action::None
                }
                _ => Action::None,
            };
        }
        match key.code {
            KeyCode::Tab => self.move_focus(1),
            KeyCode::BackTab => self.move_focus(-1),
            KeyCode::Up => self.move_focus(-1),
            KeyCode::Down => self.move_focus(1),
            KeyCode::Left if self.field() == Some(Field::Context) => self.move_context(-1),
            KeyCode::Right if self.field() == Some(Field::Context) => self.move_context(1),
            KeyCode::Left if self.field() == Some(Field::Vcpus) => self.step_vcpus(false),
            KeyCode::Right if self.field() == Some(Field::Vcpus) => self.step_vcpus(true),
            KeyCode::Left if self.field() == Some(Field::Memory) => self.step_memory(false),
            KeyCode::Right if self.field() == Some(Field::Memory) => self.step_memory(true),
            KeyCode::Left if self.field() == Some(Field::Disk) => self.step_disk(false),
            KeyCode::Right if self.field() == Some(Field::Disk) => self.step_disk(true),
            KeyCode::Enter => return self.advance(),
            KeyCode::Backspace => {
                if self.field() == Some(Field::Name) {
                    self.name_is_suggestion = false;
                }
                if let Some(value) = self.value_mut() {
                    value.pop();
                    self.error = None;
                }
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.field() == Some(Field::Name) && self.name_is_suggestion {
                    self.name.clear();
                    self.name_is_suggestion = false;
                }
                if let Some(value) = self.value_mut() {
                    value.push(character);
                    self.error = None;
                }
            }
            _ => {}
        }
        Action::None
    }

    pub(crate) fn handle_paste(&mut self, text: &str) -> Action {
        if self.stage == Stage::Fields {
            if self.field() == Some(Field::Name) && self.name_is_suggestion {
                self.name.clear();
                self.name_is_suggestion = false;
            }
            if let Some(value) = self.value_mut() {
                value.push_str(text);
                self.error = None;
            }
        }
        Action::None
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent, outer: Rect) -> Action {
        if self.stage != Stage::Fields || mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Action::None;
        }
        let layout = form_layout(outer, self.fields().len());
        let position = (mouse.column, mouse.row).into();
        if !layout.fields.contains(position) {
            return Action::None;
        }
        let focus = usize::from(mouse.row.saturating_sub(layout.fields.y));
        if focus > OK_FOCUS {
            return Action::None;
        }
        self.focus = focus;
        self.error = None;
        if focus == OK_FOCUS {
            self.advance()
        } else {
            Action::None
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, outer: Rect) {
        self.render_inner(frame, outer, true);
    }

    pub(crate) fn render_overlay(&self, frame: &mut Frame<'_>, outer: Rect) {
        self.render_inner(frame, outer, false);
    }

    fn render_inner(&self, frame: &mut Frame<'_>, outer: Rect, clear_outer: bool) {
        if clear_outer {
            frame.render_widget(Clear, outer);
        }
        let area = form_layout(outer, self.fields().len()).modal;
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::new().borders(Borders::ALL).title("Create world"),
            area,
        );
        let inner = area.inner(Margin::new(2, 1));
        if self.stage == Stage::Review {
            self.render_review(frame, inner);
        } else {
            self.render_fields(frame, inner);
        }
    }

    fn render_fields(&self, frame: &mut Frame<'_>, area: Rect) {
        let fields = self.fields();
        let rows = form_sections(area, fields.len());
        let mut lines = fields
            .iter()
            .enumerate()
            .map(|(index, field)| self.field_line(*field, index == self.focus))
            .collect::<Vec<_>>();
        lines.push(self.ok_line());
        frame.render_widget(Paragraph::new(lines), rows[0]);
        frame.render_widget(Paragraph::new(self.details()).style(muted_style()), rows[2]);
        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::new().fg(Color::LightRed)),
                rows[3],
            );
        }
        frame.render_widget(
            Paragraph::new("↑/↓ or Tab/Shift-Tab focus · ←/→ select · Enter continue · Esc cancel")
                .style(muted_style()),
            rows[4],
        );
    }

    fn render_review(&self, frame: &mut Frame<'_>, area: Rect) {
        let rows = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
        frame.render_widget(
            Paragraph::new(self.summary())
                .block(Block::new().title("Review"))
                .wrap(Wrap { trim: false }),
            rows[0],
        );
        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::new().fg(Color::LightRed)),
                rows[1],
            );
        }
        frame.render_widget(
            Paragraph::new("Enter create · b edit · Esc cancel")
                .alignment(Alignment::Center)
                .style(muted_style()),
            rows[2],
        );
    }

    fn field_line(&self, field: Field, focused: bool) -> Line<'static> {
        let marker = if focused { "› " } else { "  " };
        let label = format!("{:<LABEL_WIDTH$}", self.label(field));
        let value = self.display_value(field);
        let mut style = Style::new();
        if focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        Line::from(vec![
            Span::raw(marker),
            Span::styled(label, muted_style()),
            Span::styled(value, style),
        ])
    }

    fn ok_line(&self) -> Line<'static> {
        let style = if self.focus == OK_FOCUS {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        Line::from(Span::styled("  [ OK ]", style))
    }

    fn details(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "Git author  {} <{}>",
            self.author.name, self.author.email
        ));
        lines.join("\n")
    }

    fn summary(&self) -> String {
        format!(
            "World       {}\nContext     {}\nResources   {} CPU · {} MiB RAM · {} GiB disk\nGit author  {} <{}>",
            self.name,
            self.contexts[self.context],
            number(&self.vcpus, DEFAULT_VCPUS),
            number(&self.memory, DEFAULT_MEMORY_MIB),
            number(&self.disk, DEFAULT_DISK_GIB),
            self.author.name,
            self.author.email,
        )
    }

    fn advance(&mut self) -> Action {
        let Some(field) = self.field() else {
            return match self.input() {
                Ok(_) => {
                    self.stage = Stage::Review;
                    self.error = None;
                    Action::None
                }
                Err(error) => self.fail(error),
            };
        };
        if let Err(error) = self.validate(field) {
            return self.fail(error);
        }
        self.focus += 1;
        self.error = None;
        Action::None
    }

    fn input(&self) -> Result<Input, String> {
        for field in self.fields() {
            self.validate(*field)?;
        }
        Ok(Input {
            context: self.contexts[self.context].clone(),
            name: WorldName::parse(self.name.clone()).map_err(|error| error.to_string())?,
            vcpus: parse_number(&self.vcpus, DEFAULT_VCPUS)?,
            memory_mib: parse_number(&self.memory, DEFAULT_MEMORY_MIB)?,
            disk_gib: parse_number(&self.disk, DEFAULT_DISK_GIB)?,
            git_user_name: self.author.name.clone(),
            git_user_email: self.author.email.clone(),
        })
    }

    fn validate(&self, field: Field) -> Result<(), String> {
        match field {
            Field::Context => Ok(()),
            Field::Name => WorldName::parse(self.name.clone())
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Field::Vcpus => parse_number::<u32>(&self.vcpus, DEFAULT_VCPUS).map(|_| ()),
            Field::Memory => parse_number::<u64>(&self.memory, DEFAULT_MEMORY_MIB).map(|_| ()),
            Field::Disk => parse_number::<u64>(&self.disk, DEFAULT_DISK_GIB).map(|_| ()),
        }
    }

    fn fields(&self) -> &'static [Field] {
        &HOST_FIELDS
    }

    fn field(&self) -> Option<Field> {
        self.fields().get(self.focus).copied()
    }

    fn value_mut(&mut self) -> Option<&mut String> {
        match self.field()? {
            Field::Context => None,
            Field::Name => Some(&mut self.name),
            Field::Vcpus => Some(&mut self.vcpus),
            Field::Memory => Some(&mut self.memory),
            Field::Disk => Some(&mut self.disk),
        }
    }

    fn display_value(&self, field: Field) -> String {
        match field {
            Field::Context => format!("‹ {} ›", self.contexts[self.context]),
            Field::Name => hint(&self.name, "my-world"),
            Field::Vcpus => placeholder(&self.vcpus, &DEFAULT_VCPUS.to_string()),
            Field::Memory => placeholder(&self.memory, &DEFAULT_MEMORY_MIB.to_string()),
            Field::Disk => placeholder(&self.disk, &DEFAULT_DISK_GIB.to_string()),
        }
    }

    fn label(&self, field: Field) -> &'static str {
        match field {
            Field::Context => "Context",
            Field::Name => "World name",
            Field::Vcpus => "Virtual CPUs",
            Field::Memory => "RAM (MiB)",
            Field::Disk => "Disk (GiB)",
        }
    }

    fn move_focus(&mut self, direction: isize) {
        let len = self.fields().len() + 1;
        self.focus = if direction < 0 {
            self.focus.checked_sub(1).unwrap_or(len - 1)
        } else {
            (self.focus + 1) % len
        };
        self.error = None;
    }

    fn move_context(&mut self, direction: isize) {
        self.context = if direction < 0 {
            self.context
                .checked_sub(1)
                .unwrap_or(self.contexts.len() - 1)
        } else {
            (self.context + 1) % self.contexts.len()
        };
    }

    fn step_vcpus(&mut self, increase: bool) {
        let current = number(&self.vcpus, DEFAULT_VCPUS);
        let value = if increase {
            current.saturating_add(1)
        } else {
            current.saturating_sub(1).max(1)
        };
        self.vcpus = value.to_string();
        self.error = None;
    }

    fn step_memory(&mut self, increase: bool) {
        let current_gib = number(&self.memory, DEFAULT_MEMORY_MIB).div_ceil(1024);
        let limit = self
            .capacity()
            .map(|capacity| capacity.total.memory_mib / 1024);
        self.memory = step_power_of_two(current_gib, increase, limit)
            .saturating_mul(1024)
            .to_string();
        self.error = None;
    }

    fn step_disk(&mut self, increase: bool) {
        let current = number(&self.disk, DEFAULT_DISK_GIB);
        let limit = self.capacity().map(|capacity| capacity.total.disk_gib);
        self.disk = step_power_of_two(current, increase, limit).to_string();
        self.error = None;
    }

    fn capacity(&self) -> Option<ResourceCapacity> {
        self.capacities.get(&self.contexts[self.context]).copied()
    }

    fn fail(&mut self, error: String) -> Action {
        self.error = Some(error);
        Action::None
    }
}

struct FormLayout {
    modal: Rect,
    fields: Rect,
}

fn form_layout(outer: Rect, field_count: usize) -> FormLayout {
    let width = 82.min(outer.width);
    let height = 20.min(outer.height);
    let modal = Rect::new(
        outer.x + outer.width.saturating_sub(width) / 2,
        outer.y + outer.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = modal.inner(Margin::new(2, 1));
    FormLayout {
        modal,
        fields: form_sections(inner, field_count)[0],
    }
}

fn form_sections(area: Rect, field_count: usize) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length((field_count + 1) as u16),
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area)
}

fn muted_style() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

fn suggested_name(used_names: &std::collections::BTreeSet<String>) -> String {
    let random = u64::from_le_bytes(
        uuid::Uuid::new_v4().as_bytes()[..8]
            .try_into()
            .expect("a UUID contains eight bytes"),
    );
    suggested_name_from(used_names, random as usize)
}

fn suggested_name_from(used_names: &std::collections::BTreeSet<String>, start: usize) -> String {
    let combinations = ADJECTIVES.len() * ANIMALS.len();
    for offset in 0..combinations {
        let index = (start % combinations + offset) % combinations;
        let name = format!(
            "{}-{}",
            ADJECTIVES[index / ANIMALS.len()],
            ANIMALS[index % ANIMALS.len()]
        );
        if !used_names.contains(&name) {
            return name;
        }
    }
    (1..)
        .map(|number| format!("world-{number}"))
        .find(|name| !used_names.contains(name))
        .expect("the generated world name space is unbounded")
}

fn placeholder(value: &str, default: &str) -> String {
    if value.is_empty() {
        format!("{default} (default)")
    } else {
        value.to_owned()
    }
}

fn hint(value: &str, example: &str) -> String {
    if value.is_empty() {
        format!("<{example}>")
    } else {
        value.to_owned()
    }
}

fn number<T>(value: &str, default: T) -> T
where
    T: std::str::FromStr + Copy,
{
    value.parse().unwrap_or(default)
}

fn step_power_of_two(value: u64, increase: bool, limit: Option<u64>) -> u64 {
    let value = value.max(1);
    let stepped = if increase {
        if value.is_power_of_two() {
            value.checked_mul(2).unwrap_or(value)
        } else {
            value.checked_next_power_of_two().unwrap_or(value)
        }
    } else if value == 1 {
        1
    } else {
        1 << (u64::BITS - 1 - (value - 1).leading_zeros())
    };
    limit.filter(|limit| *limit > 0).map_or(stepped, |limit| {
        let largest_step = 1 << (u64::BITS - 1 - limit.leading_zeros());
        stepped.min(largest_step)
    })
}

fn parse_number<T>(value: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr + Copy + PartialEq + Default,
{
    let value = if value.is_empty() {
        Ok(default)
    } else {
        value
            .parse()
            .map_err(|_| "Enter a number greater than zero.")
    }?;
    if value == T::default() {
        return Err("Enter a number greater than zero.".to_owned());
    }
    Ok(value)
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "form_snapshots.rs"]
mod snapshot_tests;
