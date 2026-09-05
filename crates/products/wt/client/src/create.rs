use anyhow::{bail, Context as _, Result};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind,
};
use crossterm::execute;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use std::collections::BTreeSet;
use std::io::IsTerminal as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wt_client::config::ClientConfig;
use wt_control_protocol::{Capacity, CapacityResource, World, WorldName};

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
    pub(crate) world: World,
}

#[derive(Clone, Debug)]
pub(crate) enum FlowAction {
    None,
    Changed,
    Submit(Input),
    Cancel,
    Cancelling,
    Created(Box<Created>),
    Failed(String),
}

#[derive(Debug)]
enum Phase {
    Form,
    Creating {
        world: String,
        resources: String,
        status: String,
    },
    Capacity(String),
}

pub(crate) struct Flow {
    form: Option<Form>,
    phase: Phase,
    task: Option<Task>,
    world: Option<String>,
    resources: Option<String>,
    progress: crate::progress_toast::ProgressToast,
}

impl Flow {
    fn new(form: Form) -> Self {
        Self {
            form: Some(form),
            phase: Phase::Form,
            task: None,
            world: None,
            resources: None,
            progress: crate::progress_toast::ProgressToast::new(),
        }
    }

    pub(crate) fn start(config: &ClientConfig, input: Input) -> Result<Self> {
        let world = format!("{}.{}", input.context, input.name);
        let memory = if input.memory_mib.is_multiple_of(1024) {
            format!("{}G", input.memory_mib / 1024)
        } else {
            format!("{}MiB", input.memory_mib)
        };
        let resources = format!("{} CPU · {memory} · {}G disk", input.vcpus, input.disk_gib);
        let task = Task::start(config, input)?;
        Ok(Self {
            form: None,
            phase: Phase::Creating {
                world: world.clone(),
                resources: resources.clone(),
                status: "WT is provisioning the guest".into(),
            },
            task: Some(task),
            world: Some(world),
            resources: Some(resources),
            progress: crate::progress_toast::ProgressToast::new(),
        })
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent, _config: &ClientConfig) -> FlowAction {
        match &self.phase {
            Phase::Form => match self
                .form
                .as_mut()
                .expect("form phase has a form")
                .handle_key(key)
            {
                FormAction::None => FlowAction::None,
                FormAction::Cancel => FlowAction::Cancel,
                FormAction::Submit(input) => FlowAction::Submit(input),
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
                        status: "WT is retrying world creation".into(),
                    };
                    self.progress.reset();
                    FlowAction::None
                }
                KeyCode::Esc => {
                    if let Some(task) = &self.task {
                        task.retry(false);
                    }
                    self.phase = Phase::Creating {
                        world: self.world.clone().unwrap_or_else(|| "world".into()),
                        resources: self.resources.clone().unwrap_or_default(),
                        status: "Cancelling world creation".into(),
                    };
                    FlowAction::Cancelling
                }
                _ => FlowAction::None,
            },
        }
    }

    pub(crate) fn handle_paste(&mut self, text: &str) -> FlowAction {
        if matches!(self.phase, Phase::Form) {
            let _ = self
                .form
                .as_mut()
                .expect("form phase has a form")
                .handle_paste(text);
        }
        FlowAction::None
    }

    pub(crate) fn handle_mouse(&mut self, mouse: event::MouseEvent, area: Rect) -> FlowAction {
        if !matches!(self.phase, Phase::Form) {
            return FlowAction::None;
        }
        match self
            .form
            .as_mut()
            .expect("form phase has a form")
            .handle_mouse(mouse, area)
        {
            FormAction::None => FlowAction::None,
            FormAction::Cancel => FlowAction::Cancel,
            FormAction::Submit(_) => unreachable!("the fields stage only advances to review"),
        }
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
                self.task = None;
                FlowAction::Failed(error)
            }
        }
    }

    pub(crate) fn render(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        if let Some(form) = &self.form {
            form.render(frame, area);
        }
        self.render_status(frame, area);
    }

    pub(crate) fn render_overlay(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        if let Some(form) = &self.form {
            form.render_overlay(frame, area);
        }
        self.render_status(frame, area);
    }

    fn render_status(&self, frame: &mut ratatui::Frame<'_>, area: Rect) {
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

    pub(crate) fn status(&self) -> Option<&str> {
        match &self.phase {
            Phase::Creating { status, .. } => Some(status),
            Phase::Capacity(_) => Some("Waiting for capacity"),
            Phase::Form => None,
        }
    }

    pub(crate) fn render_progress(&self, frame: &mut ratatui::Frame<'_>, outer: Rect) {
        let Phase::Creating { world, status, .. } = &self.phase else {
            return;
        };
        self.progress
            .render(frame, outer, "World creation", world, status);
    }

    pub(crate) fn handle_progress_mouse(&mut self, event: &Event, outer: Rect) -> bool {
        if !matches!(self.phase, Phase::Creating { .. }) {
            return false;
        }
        self.progress.handle_mouse(event, outer)
    }
}

pub(crate) fn run(config: &ClientConfig) -> Result<Created> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("`wt new` requires an interactive terminal");
    }
    let inventory = crate::inventory::list_all(config);
    let used_names = inventory
        .worlds
        .into_iter()
        .map(|item| item.world.name.to_string())
        .collect();
    let mut flow = prepare(config, &used_names, inventory.capacity_by_context)?;
    let _signals = install_cancel_handlers()?;
    let mut terminal = ratatui::init();
    if let Err(error) = execute!(
        terminal.backend_mut(),
        EnableBracketedPaste,
        EnableMouseCapture
    ) {
        ratatui::restore();
        return Err(error).context("enable terminal input for world creation");
    }
    let result = run_loop(&mut terminal, &mut flow, config);
    let input_result = execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("disable terminal input for world creation");
    ratatui::restore();
    result.and_then(|created| input_result.map(|()| created))
}

pub(crate) fn prepare(
    config: &ClientConfig,
    used_names: &BTreeSet<String>,
    capacities: std::collections::BTreeMap<String, wt_control_protocol::ResourceCapacity>,
) -> Result<Flow> {
    let author = read_git_author()?;
    prepare_with_author(config, author, used_names, capacities)
}

pub(crate) fn prepare_with_author(
    config: &ClientConfig,
    author: crate::git_author::GitAuthor,
    used_names: &BTreeSet<String>,
    capacities: std::collections::BTreeMap<String, wt_control_protocol::ResourceCapacity>,
) -> Result<Flow> {
    Form::new(config, author, used_names, capacities).map(Flow::new)
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
            FlowAction::Submit(_) => unreachable!("tasks do not submit forms"),
            FlowAction::Created(created) => return Ok(*created),
            FlowAction::Failed(error) => bail!(error),
            FlowAction::Cancel => bail!("creation cancelled"),
            FlowAction::Cancelling => {}
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
            Event::Mouse(mouse) => flow.handle_mouse(mouse, terminal.size()?.into()),
            _ => FlowAction::None,
        };
        match action {
            FlowAction::Submit(input) => *flow = Flow::start(config, input)?,
            FlowAction::None => {}
            FlowAction::Changed => {}
            FlowAction::Cancel => bail!("creation cancelled"),
            FlowAction::Cancelling => {}
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

pub(crate) fn capacity_message(context: &str, name: &WorldName, capacity: &Capacity) -> String {
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
