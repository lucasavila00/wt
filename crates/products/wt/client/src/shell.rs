use anyhow::{bail, Context as _, Result};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind,
};
use crossterm::execute;
use ratatui::layout::Rect;
use std::io::{IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use wt_client::config::ClientConfig;
use wt_client::{inventory, ssh};

mod control;
mod input;
mod model;
mod render;
mod session;

use control::ControlCommand;
use model::{InputRoute, Mode, ShellModel, ShellWorld};
use session::SessionSet;
use wt_control_protocol::{ApiRequest, Operation, Response};

const BAR_HEIGHT: u16 = 1;
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const CONTEXT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub fn run(config: &ClientConfig) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("wt shell requires an interactive terminal");
    }
    let cancelled = AtomicBool::new(false);
    let report = inventory::list_all_with_timeout(config, CONTEXT_REQUEST_TIMEOUT, &cancelled);
    if !report.failures.is_empty() {
        return Err(crate::context_failures(
            "wt shell was not started because the complete world list is unavailable",
            &report.failures,
            None,
        ));
    }
    ssh::sync(config, &report.instances)?;
    let worlds = shell_worlds(&report.instances);
    let (columns, rows) = crossterm::terminal::size().context("read terminal size")?;
    let mut sessions = SessionSet::start(&worlds, world_rows(rows), columns)?;
    let mut model = ShellModel::new(worlds);
    model.set_worlds_updated_at(updated_at());
    let refresh = WorldRefresh::start(config.clone());
    let codex_refresh = CodexRefresh::start(config.clone());
    let shutdown = install_signal_handlers()?;
    let mut terminal = ratatui::init();
    if let Err(error) = execute!(
        terminal.backend_mut(),
        EnableBracketedPaste,
        EnableMouseCapture
    ) {
        ratatui::restore();
        return Err(error).context("enable terminal input for wt shell");
    }

    let result = run_loop(
        &mut terminal,
        &mut sessions,
        &mut model,
        config,
        &refresh,
        &codex_refresh,
        &shutdown,
    );
    let input_result = execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("disable terminal input for wt shell");
    ratatui::restore();
    result.and(input_result)
}

fn shell_worlds(instances: &[inventory::ContextInstance]) -> Vec<ShellWorld> {
    instances
        .iter()
        .filter(|world| ssh::has_alias(world))
        .map(|world| ShellWorld {
            identity: model::WorldIdentity {
                context: world.context.clone(),
                id: world.instance.id,
            },
            name: world.qualified_name(),
        })
        .collect()
}

struct WorldRefresh {
    updates: Receiver<WorldSnapshot>,
    generation: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

struct WorldSnapshot {
    generation: u64,
    instances: Vec<inventory::ContextInstance>,
}

struct CodexRefresh {
    updates: Receiver<Vec<control::CodexContextSnapshot>>,
    cancelled: Arc<AtomicBool>,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

struct ShellRuntime<'a> {
    config: &'a ClientConfig,
    refresh: &'a WorldRefresh,
}

impl WorldRefresh {
    fn start(config: ClientConfig) -> Self {
        let (updates_tx, updates) = mpsc::sync_channel(1);
        let (stop, stop_rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_generation = Arc::clone(&generation);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-world-refresh".into())
            .spawn(move || loop {
                if stop_rx.recv_timeout(REFRESH_INTERVAL).is_ok() {
                    break;
                }
                let generation = worker_generation.load(Ordering::Relaxed);
                let report = inventory::list_all_with_timeout(
                    &config,
                    CONTEXT_REQUEST_TIMEOUT,
                    &worker_cancelled,
                );
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                if report.failures.is_empty() {
                    match updates_tx.try_send(WorldSnapshot {
                        generation,
                        instances: report.instances,
                    }) {
                        Ok(()) | Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
            })
            .expect("start wt shell world refresh worker");
        Self {
            updates,
            generation,
            cancelled,
            stop,
            worker: Some(worker),
        }
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for WorldRefresh {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl CodexRefresh {
    fn start(config: ClientConfig) -> Self {
        let (updates_tx, updates) = mpsc::sync_channel(1);
        let (stop, stop_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-codex-refresh".into())
            .spawn(move || loop {
                let snapshot = load_codex(&config, &worker_cancelled);
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match updates_tx.try_send(snapshot) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => break,
                }
                if stop_rx.recv_timeout(REFRESH_INTERVAL).is_ok() {
                    break;
                }
            })
            .expect("start wt shell Codex refresh worker");
        Self {
            updates,
            cancelled,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for CodexRefresh {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn take_current_snapshot(
    updates: &Receiver<WorldSnapshot>,
    generation: u64,
) -> Option<WorldSnapshot> {
    updates
        .try_iter()
        .last()
        .filter(|snapshot| snapshot.generation == generation)
}

fn load_codex(config: &ClientConfig, cancelled: &AtomicBool) -> Vec<control::CodexContextSnapshot> {
    let request = ApiRequest::new(Operation::ListCodexSessions);
    config
        .contexts
        .iter()
        .take_while(|_| !cancelled.load(Ordering::Relaxed))
        .map(|context| {
            match wt_client::transport::call_with_timeout_until(
                context,
                &request,
                CONTEXT_REQUEST_TIMEOUT,
                cancelled,
            ) {
                Ok(Response::CodexSessions { sessions }) => {
                    control::CodexContextSnapshot::Sessions {
                        context: context.name.clone(),
                        sessions,
                    }
                }
                Ok(_) => control::CodexContextSnapshot::Failure {
                    context: context.name.clone(),
                    message: wt_client::transport::wrong_response(context, "list Codex sessions")
                        .to_string(),
                },
                Err(error) => control::CodexContextSnapshot::Failure {
                    context: context.name.clone(),
                    message: error.to_string(),
                },
            }
        })
        .collect()
}

fn updated_at() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .expect("zero is a valid nanosecond")
        .format(&Rfc3339)
        .expect("UTC timestamps support RFC 3339")
}

fn install_signal_handlers() -> Result<Arc<AtomicBool>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))
        .context("install wt shell SIGINT handler")?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))
        .context("install wt shell SIGTERM handler")?;
    Ok(shutdown)
}

fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    config: &ClientConfig,
    refresh: &WorldRefresh,
    codex_refresh: &CodexRefresh,
    shutdown: &AtomicBool,
) -> Result<()> {
    let mut redraw = true;
    let mut creation = None;
    let mut creation_error = None;
    let runtime = ShellRuntime { config, refresh };
    while !shutdown.load(Ordering::Relaxed) {
        if creation.is_none() {
            if let Some(snapshot) =
                take_current_snapshot(&refresh.updates, refresh.generation.load(Ordering::Relaxed))
            {
                if ssh::sync(config, &snapshot.instances).is_ok() {
                    let worlds = shell_worlds(&snapshot.instances);
                    let area: Rect = terminal
                        .size()
                        .context("read wt shell terminal area")?
                        .into();
                    sessions.reconcile(&worlds, world_rows(area.height), area.width)?;
                    model.reconcile_worlds(worlds);
                    model.set_worlds_updated_at(updated_at());
                    redraw = true;
                }
            }
        }
        if let Some(codex) = codex_refresh.updates.try_iter().last() {
            model.set_codex(codex, updated_at());
            redraw = true;
        }
        let (output_changed, clipboard_writes) = sessions.drain_output(model.active());
        redraw |= output_changed;
        for sequence in clipboard_writes {
            terminal
                .backend_mut()
                .write_all(&sequence)
                .context("relay world clipboard write")?;
        }
        if let Some(action) = creation.as_mut().map(crate::create::Flow::poll) {
            redraw |= apply_creation_action(
                action,
                &mut creation,
                &mut creation_error,
                sessions,
                model,
                refresh,
                terminal
                    .size()
                    .context("read wt shell terminal area")?
                    .into(),
            )?;
        }
        if redraw {
            let screen = model.has_worlds().then(|| sessions.screen(model.active()));
            terminal.draw(|frame| {
                render::draw(
                    frame,
                    screen,
                    model,
                    creation.as_ref(),
                    creation_error.as_deref(),
                )
            })?;
            redraw = false;
        }
        if !event::poll(Duration::from_millis(16)).context("poll terminal input")? {
            continue;
        }
        loop {
            let area = terminal
                .size()
                .context("read wt shell terminal area")?
                .into();
            redraw |= dispatch_event(
                event::read().context("read terminal input")?,
                sessions,
                model,
                area,
                &runtime,
                &mut creation,
                &mut creation_error,
            )?;
            if model.should_quit() {
                return Ok(());
            }
            if !event::poll(Duration::ZERO).context("poll pending terminal input")? {
                break;
            }
        }
    }
    Ok(())
}

fn dispatch_event(
    event: Event,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    area: Rect,
    runtime: &ShellRuntime<'_>,
    creation: &mut Option<crate::create::Flow>,
    creation_error: &mut Option<String>,
) -> Result<bool> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if matches!(key.code, crossterm::event::KeyCode::F(5 | 6)) {
                if model.handle_key(key) == InputRoute::World {
                    let screen = sessions.screen(model.active());
                    if let Some(bytes) = input::encode_key(key, screen.application_cursor())? {
                        sessions.write(model.active(), &bytes)?;
                    }
                }
                return Ok(true);
            }
            if model.mode() == Mode::Control {
                if creation_error.is_some()
                    && matches!(
                        key.code,
                        crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Esc
                    )
                {
                    creation_error.take();
                    return Ok(true);
                }
                if let Some(flow) = creation.as_mut() {
                    let action = flow.handle_key(key, runtime.config);
                    let _ = apply_creation_action(
                        action,
                        creation,
                        creation_error,
                        sessions,
                        model,
                        runtime.refresh,
                        area,
                    )?;
                    return Ok(true);
                }
            }
            match model.handle_key(key) {
                InputRoute::World => {
                    let screen = sessions.screen(model.active());
                    if let Some(bytes) = input::encode_key(key, screen.application_cursor())? {
                        sessions.write(model.active(), &bytes)?;
                    }
                }
                InputRoute::Command(command) => {
                    start_creation(
                        command,
                        runtime.config,
                        runtime.refresh,
                        creation,
                        creation_error,
                    );
                }
                InputRoute::Consumed => {}
            }
            Ok(true)
        }
        Event::Paste(text) if model.mode() == Mode::Control && creation.is_some() => {
            if let Some(flow) = creation.as_mut() {
                let _ = flow.handle_paste(&text);
            }
            Ok(true)
        }
        Event::Paste(text) if model.mode() == Mode::World => {
            let bracketed = sessions.screen(model.active()).bracketed_paste();
            sessions.write(model.active(), &input::encode_paste(&text, bracketed))?;
            Ok(true)
        }
        Event::Mouse(mouse) if model.mode().forwards_mouse() => {
            if let Some(mouse) = world_mouse(mouse, area) {
                let screen = sessions.screen(model.active());
                if let Some(bytes) = input::encode_mouse(
                    mouse,
                    screen.mouse_protocol_mode(),
                    screen.mouse_protocol_encoding(),
                ) {
                    sessions.write(model.active(), &bytes)?;
                }
            }
            Ok(false)
        }
        Event::Mouse(mouse) if model.mode() == Mode::Control => {
            if creation.is_none() {
                if let Some(command) = model.handle_mouse(mouse, area) {
                    start_creation(
                        command,
                        runtime.config,
                        runtime.refresh,
                        creation,
                        creation_error,
                    );
                }
            }
            Ok(true)
        }
        Event::Resize(columns, rows) => {
            sessions.resize(world_rows(rows), columns)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}
fn world_rows(terminal_rows: u16) -> u16 {
    terminal_rows.saturating_sub(BAR_HEIGHT).max(1)
}

fn world_area(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(BAR_HEIGHT),
        area.width,
        area.height.saturating_sub(BAR_HEIGHT),
    )
}

fn world_mouse(
    mut mouse: crossterm::event::MouseEvent,
    area: Rect,
) -> Option<crossterm::event::MouseEvent> {
    let world = world_area(area);
    if mouse.column < world.x
        || mouse.column >= world.x.saturating_add(world.width)
        || mouse.row < world.y
        || mouse.row >= world.y.saturating_add(world.height)
    {
        return None;
    }
    mouse.column -= world.x;
    mouse.row -= world.y;
    Some(mouse)
}

fn start_creation(
    command: ControlCommand,
    config: &ClientConfig,
    refresh: &WorldRefresh,
    creation: &mut Option<crate::create::Flow>,
    error: &mut Option<String>,
) {
    let kind = match command {
        ControlCommand::NewDev => Ok(crate::create::Kind::Dev),
        ControlCommand::NewHost => crate::host::default_input().map(crate::create::Kind::Host),
    };
    match kind.and_then(|kind| crate::create::prepare(config, kind)) {
        Ok(flow) => {
            refresh.invalidate();
            *creation = Some(flow);
            *error = None;
        }
        Err(cause) => *error = Some(format!("{cause:#}")),
    }
}

fn apply_creation_action(
    action: crate::create::FlowAction,
    creation: &mut Option<crate::create::Flow>,
    error: &mut Option<String>,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    refresh: &WorldRefresh,
    area: ratatui::layout::Rect,
) -> Result<bool> {
    match action {
        crate::create::FlowAction::None => Ok(false),
        crate::create::FlowAction::Changed => Ok(true),
        crate::create::FlowAction::Cancel => {
            creation.take();
            Ok(true)
        }
        crate::create::FlowAction::Failed(message) => {
            creation.take();
            *error = Some(message);
            Ok(true)
        }
        crate::create::FlowAction::Created(created) => {
            refresh.invalidate();
            let world = ShellWorld {
                identity: model::WorldIdentity {
                    context: created.context.clone(),
                    id: created.instance.id,
                },
                name: format!("{}.{}", created.context, created.instance.name),
            };
            if model.world_index(&world.identity).is_none() {
                sessions.add_world(&world, world_rows(area.height), area.width)?;
            }
            model.activate_world(world);
            creation.take();
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    #[test]
    fn world_view_reserves_the_top_row() {
        assert_eq!(world_rows(24), 23);
        assert_eq!(world_rows(1), 1);
        assert_eq!(world_area(Rect::new(0, 0, 80, 24)), Rect::new(0, 1, 80, 23));
    }

    #[test]
    fn mouse_input_skips_the_bar_and_is_translated_to_world_rows() {
        let area = Rect::new(0, 0, 80, 24);

        assert_eq!(world_mouse(mouse(4, 0), area), None);
        assert_eq!(world_mouse(mouse(4, 1), area).unwrap().row, 0);
        assert_eq!(world_mouse(mouse(4, 23), area).unwrap().row, 22);
    }

    fn mouse(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn local_mutation_invalidates_an_older_refresh() {
        let (sender, updates) = mpsc::sync_channel(1);
        sender
            .send(WorldSnapshot {
                generation: 4,
                instances: Vec::new(),
            })
            .unwrap();

        assert!(take_current_snapshot(&updates, 5).is_none());
    }
}
