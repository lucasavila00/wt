use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use wt_client::config::ClientConfig;
use wt_control_protocol::{CreateApplication, InstanceName};

use super::Kind;
use crate::git_author::GitAuthor;

const DEFAULT_VCPUS: u32 = 2;
const DEFAULT_MEMORY_MIB: u64 = 4096;
const DEFAULT_DISK_GIB: u64 = 32;
const LABEL_WIDTH: usize = 16;
const HOST_FIELDS: [Field; 5] = [
    Field::Context,
    Field::Name,
    Field::Vcpus,
    Field::Memory,
    Field::Disk,
];

#[derive(Clone, Debug)]
pub(crate) struct Input {
    pub(crate) context: String,
    pub(crate) name: InstanceName,
    pub(crate) vcpus: u32,
    pub(crate) memory_mib: u64,
    pub(crate) disk_gib: u64,
    pub(crate) ssh_authorized_keys: Vec<String>,
    pub(crate) git_user_name: String,
    pub(crate) git_user_email: String,
    pub(crate) application: CreateApplication,
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
    kind: Kind,
    contexts: Vec<String>,
    context: usize,
    name: String,
    vcpus: String,
    memory: String,
    disk: String,
    author: GitAuthor,
    keys: Vec<(String, String)>,
    focus: usize,
    stage: Stage,
    error: Option<String>,
}

impl Form {
    pub(crate) fn new(
        config: &ClientConfig,
        kind: Kind,
        author: GitAuthor,
        keys: Vec<(String, String)>,
    ) -> anyhow::Result<Self> {
        if config.contexts.is_empty() {
            anyhow::bail!("no contexts are configured");
        }
        Ok(Self {
            kind,
            contexts: config
                .contexts
                .iter()
                .map(|context| context.name.clone())
                .collect(),
            context: 0,
            name: String::new(),
            vcpus: String::new(),
            memory: String::new(),
            disk: String::new(),
            author,
            keys,
            focus: 0,
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
            KeyCode::Left if self.field() == Field::Context => self.move_context(-1),
            KeyCode::Right if self.field() == Field::Context => self.move_context(1),
            KeyCode::Enter => return self.advance(),
            KeyCode::Backspace => {
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
            if let Some(value) = self.value_mut() {
                value.push_str(text);
                self.error = None;
            }
        }
        Action::None
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, outer: Rect) {
        frame.render_widget(Clear, outer);
        frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), outer);
        let width = 82.min(outer.width);
        let height = 20.min(outer.height);
        let area = Rect::new(
            outer.x + outer.width.saturating_sub(width) / 2,
            outer.y + outer.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(
            Block::new()
                .borders(Borders::ALL)
                .title(format!("Create {} world", self.kind_name())),
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
        let rows = Layout::vertical([
            Constraint::Length(fields.len() as u16),
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
        let lines = fields
            .iter()
            .enumerate()
            .map(|(index, field)| self.field_line(*field, index == self.focus))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), rows[0]);
        frame.render_widget(
            Paragraph::new(self.details()).style(Style::new().fg(Color::DarkGray)),
            rows[2],
        );
        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::new().fg(Color::LightRed)),
                rows[3],
            );
        }
        frame.render_widget(
            Paragraph::new("↑/↓ or Tab/Shift-Tab focus · ←/→ select · Enter continue · Esc cancel")
                .style(Style::new().fg(Color::DarkGray)),
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
                .style(Style::new().fg(Color::DarkGray)),
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
            Span::styled(label, Style::new().fg(Color::DarkGray)),
            Span::styled(value, style),
        ])
    }

    fn details(&self) -> String {
        let mut lines = Vec::new();
        let Kind::Host(input) = &self.kind;
        lines.push(format!("Cloud-init  {}", input.user_data_path.display()));
        lines.push(format!(
            "Git author  {} <{}>",
            self.author.name, self.author.email
        ));
        lines.push(format!("SSH keys    {} discovered", self.keys.len()));
        lines.join("\n")
    }

    fn summary(&self) -> String {
        let Kind::Host(input) = &self.kind;
        let application = format!("Cloud-init  {}", input.user_data_path.display());
        let mut summary = format!(
            "World       {}\nContext     {}\nKind        {}\n{}\nResources   {} CPU · {} MiB RAM · {} GiB disk\nGit author  {} <{}>\nSSH keys    {}",
            self.name,
            self.contexts[self.context],
            self.kind_name(),
            application,
            number(&self.vcpus, DEFAULT_VCPUS),
            number(&self.memory, DEFAULT_MEMORY_MIB),
            number(&self.disk, DEFAULT_DISK_GIB),
            self.author.name,
            self.author.email,
            self.keys.len(),
        );
        for (_, fingerprint) in &self.keys {
            summary.push_str("\n            ");
            summary.push_str(fingerprint);
        }
        summary
    }

    fn advance(&mut self) -> Action {
        if let Err(error) = self.validate(self.field()) {
            return self.fail(error);
        }
        if self.focus + 1 < self.fields().len() {
            self.focus += 1;
            self.error = None;
            return Action::None;
        }
        match self.input() {
            Ok(_) => {
                self.stage = Stage::Review;
                self.error = None;
                Action::None
            }
            Err(error) => self.fail(error),
        }
    }

    fn input(&self) -> Result<Input, String> {
        for field in self.fields() {
            self.validate(*field)?;
        }
        let Kind::Host(input) = &self.kind;
        let application = CreateApplication::Host {
            user_data: input.user_data.clone(),
        };
        Ok(Input {
            context: self.contexts[self.context].clone(),
            name: InstanceName::parse(self.name.clone()).map_err(|error| error.to_string())?,
            vcpus: parse_number(&self.vcpus, DEFAULT_VCPUS)?,
            memory_mib: parse_number(&self.memory, DEFAULT_MEMORY_MIB)?,
            disk_gib: parse_number(&self.disk, DEFAULT_DISK_GIB)?,
            ssh_authorized_keys: self.keys.iter().map(|(key, _)| key.clone()).collect(),
            git_user_name: self.author.name.clone(),
            git_user_email: self.author.email.clone(),
            application,
        })
    }

    fn validate(&self, field: Field) -> Result<(), String> {
        match field {
            Field::Context => Ok(()),
            Field::Name => InstanceName::parse(self.name.clone())
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

    fn field(&self) -> Field {
        self.fields()[self.focus]
    }

    fn value_mut(&mut self) -> Option<&mut String> {
        match self.field() {
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
        let len = self.fields().len();
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

    fn kind_name(&self) -> &'static str {
        "host"
    }

    fn fail(&mut self, error: String) -> Action {
        self.error = Some(error);
        Action::None
    }
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
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use wt_client::config::{Context, ContextKind};

    fn form(kind: Kind) -> Form {
        Form::new(
            &ClientConfig {
                contexts: vec![
                    Context {
                        name: "local".into(),
                        kind: ContextKind::BareMetalLocal,
                    },
                    Context {
                        name: "lab".into(),
                        kind: ContextKind::BareMetalLocal,
                    },
                ],
            },
            kind,
            GitAuthor {
                name: "Test User".into(),
                email: "test@example.com".into(),
            },
            vec![("ssh-ed25519 key".into(), "SHA256:key".into())],
        )
        .unwrap()
    }

    fn host_kind() -> Kind {
        Kind::Host(crate::host::Input {
            user_data: "#cloud-config\n".into(),
            user_data_path: "/home/test/.config/wt/cloud-init.yaml".into(),
        })
    }

    #[test]
    fn host_form_validates_and_builds_the_request_input() {
        let mut form = form(host_kind());
        form.name = "demo".into();
        form.context = 1;

        let input = form.input().unwrap();

        assert_eq!(input.context, "lab");
        assert_eq!(input.name.as_str(), "demo");
        assert_eq!(input.vcpus, DEFAULT_VCPUS);
        assert!(matches!(
            input.application,
            CreateApplication::Host { user_data } if user_data == "#cloud-config\n"
        ));
    }

    #[test]
    fn invalid_values_stay_in_the_form() {
        let mut form = form(host_kind());
        form.focus = 1;

        assert!(matches!(
            form.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::None
        ));
        assert!(form.error.as_deref().unwrap().contains("instance name"));
        assert_eq!(form.focus, 1);
    }

    #[test]
    fn terminal_sequence_reaches_confirmation() {
        let mut form = form(host_kind());
        let mut action = Action::None;
        for character in "\nrepo-feature\n\n\n\n\n".chars() {
            let code = if character == '\n' {
                KeyCode::Enter
            } else {
                KeyCode::Char(character)
            };
            action = form.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
        assert!(matches!(action, Action::Submit(_)));
    }

    #[test]
    fn renders_the_editing_form() {
        let backend = TestBackend::new(84, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        let form = form(host_kind());

        terminal
            .draw(|frame| form.render(frame, frame.area()))
            .unwrap();

        insta::assert_debug_snapshot!("world_creation_form", terminal.backend().buffer());
    }
}
