use anyhow::{bail, Context as _, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEventKind};
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ssh_key::{HashAlg, PublicKey};
use std::collections::BTreeSet;
use std::io::IsTerminal as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use wt_client::config::ClientConfig;
use wt_control_protocol::{Capacity, CapacityResource, Instance, InstanceName};

use crate::git_author::read_git_author;

mod form;
mod task;

pub(crate) use form::{Action as FormAction, Form, Input};
use task::{Task, TaskEvent};

const INPUT_POLL: Duration = Duration::from_millis(50);
static CANCELLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub(crate) struct Created {
    pub(crate) context: String,
    pub(crate) instance: Instance,
}

#[derive(Clone, Debug)]
pub(crate) enum FlowAction {
    None,
    Changed,
    Cancel,
    Created(Box<Created>),
    Failed(String),
}

#[derive(Debug)]
enum Phase {
    Form,
    Creating {
        world: String,
        resources: String,
        started: Instant,
        status: String,
    },
    Capacity(String),
    Failed(String),
}

pub(crate) struct Flow {
    form: Form,
    phase: Phase,
    task: Option<Task>,
    world: Option<String>,
    resources: Option<String>,
    progress_visible: bool,
}

impl Flow {
    fn new(form: Form) -> Self {
        Self {
            form,
            phase: Phase::Form,
            task: None,
            world: None,
            resources: None,
            progress_visible: true,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent, config: &ClientConfig) -> FlowAction {
        match &self.phase {
            Phase::Form => match self.form.handle_key(key) {
                FormAction::None => FlowAction::None,
                FormAction::Cancel => FlowAction::Cancel,
                FormAction::Submit(input) => {
                    let world = format!("{}.{}", input.context, input.name);
                    let memory = if input.memory_mib.is_multiple_of(1024) {
                        format!("{}G", input.memory_mib / 1024)
                    } else {
                        format!("{}MiB", input.memory_mib)
                    };
                    let resources =
                        format!("{} CPU · {memory} · {}G disk", input.vcpus, input.disk_gib);
                    match Task::start(config, input) {
                        Ok(task) => {
                            self.task = Some(task);
                            self.world = Some(world.clone());
                            self.resources = Some(resources.clone());
                            self.progress_visible = true;
                            self.phase = Phase::Creating {
                                world,
                                resources,
                                started: Instant::now(),
                                status: "WT is provisioning the guest".into(),
                            };
                            FlowAction::Changed
                        }
                        Err(error) => {
                            self.phase = Phase::Failed(format!("{error:#}"));
                            FlowAction::Changed
                        }
                    }
                }
            },
            Phase::Creating { .. } => FlowAction::None,
            Phase::Capacity(_) => match key.code {
                KeyCode::Enter => {
                    if let Some(task) = &self.task {
                        task.retry(true);
                    }
                    self.phase = Phase::Creating {
                        world: self.world.clone().unwrap_or_else(|| "world".into()),
                        resources: self.resources.clone().unwrap_or_default(),
                        started: Instant::now(),
                        status: "WT is retrying world creation".into(),
                    };
                    self.progress_visible = true;
                    FlowAction::None
                }
                KeyCode::Esc => {
                    if let Some(task) = &self.task {
                        task.retry(false);
                    }
                    FlowAction::Cancel
                }
                _ => FlowAction::None,
            },
            Phase::Failed(error) => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    FlowAction::Failed(error.clone())
                } else {
                    FlowAction::None
                }
            }
        }
    }

    pub(crate) fn handle_paste(&mut self, text: &str) -> FlowAction {
        if matches!(self.phase, Phase::Form) {
            let _ = self.form.handle_paste(text);
        }
        FlowAction::None
    }

    pub(crate) fn poll(&mut self) -> FlowAction {
        let Some(event) = self.task.as_ref().and_then(Task::poll) else {
            return FlowAction::None;
        };
        match event {
            TaskEvent::Progress(status) => {
                if let Phase::Creating {
                    status: current, ..
                } = &mut self.phase
                {
                    *current = status;
                }
                FlowAction::Changed
            }
            TaskEvent::Capacity(message) => {
                self.phase = Phase::Capacity(message);
                FlowAction::Changed
            }
            TaskEvent::Finished(Ok(created)) => FlowAction::Created(created),
            TaskEvent::Finished(Err(error)) => {
                self.phase = Phase::Failed(error);
                self.task = None;
                FlowAction::Changed
            }
        }
    }

    pub(crate) fn render(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        self.form.render(frame, area);
        let (title, message, help) = match &self.phase {
            Phase::Form => return,
            Phase::Creating { .. } => {
                self.render_progress(frame, area);
                return;
            }
            Phase::Capacity(message) => (
                "World capacity is full",
                message.as_str(),
                "Enter retry · Esc cancel",
            ),
            Phase::Failed(message) => (
                "Creation did not complete",
                message.as_str(),
                "Enter/Esc close",
            ),
        };
        render_status(frame, area, title, message, help);
    }

    pub(crate) fn blocks_input(&self) -> bool {
        !matches!(self.phase, Phase::Creating { .. })
    }

    pub(crate) fn creating_world(&self) -> Option<(&str, &str)> {
        match &self.phase {
            Phase::Creating {
                world, resources, ..
            } => Some((world, resources)),
            _ => None,
        }
    }

    pub(crate) fn render_progress(&self, frame: &mut ratatui::Frame<'_>, outer: Rect) {
        if !self.progress_visible {
            return;
        }
        let Phase::Creating {
            world,
            started,
            status,
            ..
        } = &self.phase
        else {
            return;
        };
        let elapsed_duration = started.elapsed();
        let elapsed = elapsed_duration.as_secs();
        const GRADIENT: [u8; 12] = [24, 25, 31, 37, 43, 42, 36, 30, 24, 60, 54, 53];
        let animation_tick = elapsed_duration.as_millis() as usize / 25;
        let spinner = ["", "", "", ""][(animation_tick / 2) % 4];
        let Some(area) = progress_area(outer) else {
            return;
        };
        frame.render_widget(Clear, area);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(" World creation ")
            .title(Line::from("×").alignment(Alignment::Right));
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(format!("{spinner} {world}\n{status}\n{elapsed}s elapsed"))
                .wrap(Wrap { trim: false })
                .style(Style::new().fg(Color::Indexed(
                    GRADIENT[(animation_tick / 4) % GRADIENT.len()],
                ))),
            area.inner(Margin::new(1, 1)),
        );
    }

    pub(crate) fn handle_progress_mouse(&mut self, event: &Event, outer: Rect) -> bool {
        if !self.progress_visible || !matches!(self.phase, Phase::Creating { .. }) {
            return false;
        }
        let Event::Mouse(mouse) = event else {
            return false;
        };
        let Some(area) = progress_area(outer) else {
            return false;
        };
        let position = (mouse.column, mouse.row).into();
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && progress_dismiss_area(area).contains(position)
        {
            self.progress_visible = false;
            return true;
        }
        area.contains(position)
    }
}

fn progress_area(outer: Rect) -> Option<Rect> {
    let width = 44.min(outer.width.saturating_sub(2));
    if width < 24 || outer.height < 7 {
        return None;
    }
    Some(Rect::new(
        outer.right().saturating_sub(1).saturating_sub(width),
        outer.y.saturating_add(1),
        width,
        6,
    ))
}

fn progress_dismiss_area(area: Rect) -> Rect {
    Rect::new(area.right().saturating_sub(2), area.y, 1, 1)
}

pub(crate) fn run(config: &ClientConfig) -> Result<Created> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("`wt new` requires an interactive terminal");
    }
    let mut flow = prepare(config)?;
    let _signals = install_cancel_handlers()?;
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut flow, config);
    ratatui::restore();
    result
}

pub(crate) fn prepare(config: &ClientConfig) -> Result<Flow> {
    let author = read_git_author()?;
    let keys = discover_public_keys()?;
    Form::new(config, author, keys).map(Flow::new)
}

fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    flow: &mut Flow,
    config: &ClientConfig,
) -> Result<Created> {
    loop {
        if CANCELLED.load(Ordering::SeqCst) {
            bail!("creation cancelled");
        }
        match flow.poll() {
            FlowAction::Created(created) => return Ok(*created),
            FlowAction::Failed(error) => bail!(error),
            FlowAction::Cancel => bail!("creation cancelled"),
            FlowAction::None => {}
            FlowAction::Changed => {}
        }
        terminal.draw(|frame| flow.render(frame, frame.area()))?;
        if !event::poll(INPUT_POLL).context("poll world creation input")? {
            continue;
        }
        let action = match event::read().context("read world creation input")? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                flow.handle_key(key, config)
            }
            Event::Paste(text) => flow.handle_paste(&text),
            _ => FlowAction::None,
        };
        match action {
            FlowAction::None => {}
            FlowAction::Changed => {}
            FlowAction::Cancel => bail!("creation cancelled"),
            FlowAction::Created(created) => return Ok(*created),
            FlowAction::Failed(error) => bail!(error),
        }
    }
}

fn render_status(
    frame: &mut ratatui::Frame<'_>,
    outer: Rect,
    title: &str,
    message: &str,
    help: &str,
) {
    let width = 64.min(outer.width);
    let height = 10.min(outer.height);
    let area = Rect::new(
        outer.x + outer.width.saturating_sub(width) / 2,
        outer.y + outer.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(Block::new().borders(Borders::ALL).title(title), area);
    let inner = area.inner(Margin::new(2, 1));
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: false }), rows[0]);
    frame.render_widget(
        Paragraph::new(help)
            .alignment(Alignment::Center)
            .style(Style::new().fg(Color::DarkGray)),
        rows[1],
    );
}

pub(crate) fn capacity_message(context: &str, name: &InstanceName, capacity: &Capacity) -> String {
    let (resource, unit) = match capacity.resource {
        CapacityResource::Cpu => ("CPU", "CPU"),
        CapacityResource::Memory => ("memory", "MiB"),
        CapacityResource::Disk => ("disk", "GiB"),
    };
    format!(
        "{context} has {} {unit} of {} {unit} world {resource} reserved; {name} requests {} {unit}.\nFree capacity with `wt ls` and `wt stop CONTEXT.WORLD` or `wt rm CONTEXT.WORLD`.",
        capacity.reserved, capacity.total, capacity.requested
    )
}

fn discover_public_keys() -> Result<Vec<(String, String)>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let directory = home.join(".ssh");
    let entries = std::fs::read_dir(&directory)
        .with_context(|| format!("read SSH directory {}", directory.display()))?;
    let mut keys = BTreeSet::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("read {} entry", directory.display()))?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("pub")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let value = std::fs::read_to_string(entry.path())
            .with_context(|| format!("read public key {}", entry.path().display()))?;
        let mut key = PublicKey::from_openssh(value.trim())
            .with_context(|| format!("parse public key {}", entry.path().display()))?;
        key.set_comment("");
        keys.insert(key.to_openssh()?);
    }
    if keys.is_empty() {
        bail!("no valid public keys found in {}", directory.display());
    }
    keys.into_iter()
        .map(|key| {
            let parsed = PublicKey::from_openssh(&key)?;
            Ok((key, parsed.fingerprint(HashAlg::Sha256).to_string()))
        })
        .collect()
}

extern "C" fn cancel_prompt(_: i32) {
    CANCELLED.store(true, Ordering::SeqCst);
}

struct SignalGuard(Vec<(Signal, SigAction)>);

impl Drop for SignalGuard {
    fn drop(&mut self) {
        for (signal, action) in &self.0 {
            // SAFETY: restore the action returned by the matching sigaction call.
            let _ = unsafe { signal::sigaction(*signal, action) };
        }
    }
}

fn install_cancel_handlers() -> Result<SignalGuard> {
    CANCELLED.store(false, Ordering::SeqCst);
    let action = SigAction::new(
        SigHandler::Handler(cancel_prompt),
        SaFlags::empty(),
        SigSet::empty(),
    );
    let mut previous = Vec::new();
    for signal in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the handler only stores to a lock-free atomic.
        let old = unsafe { signal::sigaction(signal, &action) }
            .with_context(|| format!("install {signal} handler"))?;
        previous.push((signal, old));
    }
    Ok(SignalGuard(previous))
}
