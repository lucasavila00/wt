use anyhow::{bail, Context as _, Result};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind,
};
use crossterm::execute;
use ratatui::layout::Rect;
use std::io::{IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wt_client::config::ClientConfig;
use wt_client::{inventory, ssh};

mod action;
mod action_queue;
mod activity;
mod bar;
mod clipboard;
mod control;
mod control_overlay;
mod delete;
mod focus;
mod git_activity;
mod input;
mod lifecycle;
mod live;
mod model;
mod pane;
mod refresh;
mod refresh_status;
mod render;
mod scrollbar;
mod session;
mod shutdown;
mod terminal_view;
mod world_card;
mod world_menu;
use control::ControlCommand;
use lifecycle::start_control_command;
use model::{InputRoute, Mode, ShellModel, ShellWorld};
use refresh::{take_current_snapshot, PaneRefresh, WorldRefresh};
use session::SessionSet;
use shutdown::{log_running_work, request_close};

const BAR_HEIGHT: u16 = 1;
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const CONTEXT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub fn run(config: &ClientConfig, test_server: bool) -> Result<()> {
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
    ssh::sync(config, &report.worlds)?;
    let worlds = shell_worlds(&report.worlds);
    let (columns, rows) = crossterm::terminal::size().context("read terminal size")?;
    let mut model = ShellModel::new(worlds);
    let area = Rect::new(0, 0, columns, rows);
    let (session_rows, session_columns) = session_viewport(&model, area);
    let mut sessions = SessionSet::start(model.worlds(), session_rows, session_columns)?;
    model.control_mut().set_capacity(report.capacity);
    model.set_test_server(test_server);
    model.finish_worlds_refresh(Ok(refresh::updated_at()));
    let refresh = WorldRefresh::start(config.clone());
    let pane_refresh = PaneRefresh::start(config.clone());
    let git_activity = git_activity::Refresh::start(config.clone(), report.worlds);
    let focus = focus::FocusWorker::default();
    let git_author = crate::git_author::read_git_author().map_err(|error| format!("{error:#}"));
    let runtime = ShellRuntime {
        config,
        refresh: &refresh,
        pane_refresh: &pane_refresh,
        git_activity: &git_activity,
        focus: &focus,
        git_author: &git_author,
    };
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
        &runtime,
        &shutdown,
    );
    let input_result = execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("disable terminal input for wt shell");
    ratatui::restore();
    if let Ok(running) = &result {
        log_running_work(running);
    }
    result.and(input_result).map(|_| ())
}

fn shell_worlds(worlds: &[inventory::ContextWorld]) -> Vec<ShellWorld> {
    worlds
        .iter()
        .map(pane::ShellWorld::from_inventory)
        .collect()
}

struct ShellRuntime<'a> {
    config: &'a ClientConfig,
    refresh: &'a WorldRefresh,
    pane_refresh: &'a PaneRefresh,
    git_activity: &'a git_activity::Refresh,
    focus: &'a focus::FocusWorker,
    git_author: &'a Result<crate::git_author::GitAuthor, String>,
}

#[derive(Default)]
struct ControlFlows {
    creation: Option<crate::create::Flow>,
    action_error: Option<String>,
    deletion: Option<delete::Flow>,
    actions: action_queue::ShellActionQueue,
    task: Option<action::Task>,
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
    runtime: &ShellRuntime<'_>,
    shutdown: &AtomicBool,
) -> Result<Vec<String>> {
    let mut redraw = true;
    let mut flows = ControlFlows::default();
    while !shutdown.load(Ordering::Relaxed) {
        let area: Rect = terminal
            .size()
            .context("read wt shell terminal area")?
            .into();
        let (rows, columns) = session_viewport(model, area);
        sessions.resize(model.mode(), model.active(), rows, columns)?;
        let (output_changed, clipboard_writes) = sessions.drain_output(model.active());
        redraw |= output_changed;
        for sequence in clipboard_writes {
            terminal
                .backend_mut()
                .write_all(&sequence)
                .context("relay world clipboard write")?;
        }
        if !flows.actions.has_work() {
            if let Some(snapshot) = take_current_snapshot(
                &runtime.refresh.updates,
                runtime.refresh.generation.load(Ordering::Relaxed),
            ) {
                if !snapshot.failures.is_empty() {
                    model.finish_worlds_refresh(Err(snapshot.failures));
                    redraw = true;
                } else if let Some(error) = snapshot.ssh_sync_error {
                    model.finish_worlds_refresh(Err(vec![error]));
                    redraw = true;
                } else {
                    runtime.git_activity.reconcile(snapshot.worlds.clone());
                    let worlds = shell_worlds(&snapshot.worlds);
                    let area: Rect = terminal
                        .size()
                        .context("read wt shell terminal area")?
                        .into();
                    let (rows, columns) = session_viewport(model, area);
                    sessions.reconcile(&worlds, rows, columns)?;
                    model.reconcile_worlds(worlds);
                    model.control_mut().set_capacity(snapshot.capacity);
                    model.finish_worlds_refresh(Ok(refresh::updated_at()));
                    redraw = true;
                }
            }
        } else {
            let _ = runtime.refresh.updates.try_iter().last();
        }
        if !flows.actions.has_work() {
            if let Some(snapshot) = runtime.pane_refresh.updates.try_iter().last() {
                let area = terminal
                    .size()
                    .context("read wt shell terminal area")?
                    .into();
                let snapshot = pane::cards(snapshot, model.worlds());
                redraw |= model.control_mut().apply_pane_refresh(
                    snapshot.cards,
                    snapshot.failures,
                    refresh::updated_at(),
                    area,
                );
            }
        } else {
            let _ = runtime.pane_refresh.updates.try_iter().last();
        }
        for update in runtime.git_activity.updates.try_iter() {
            redraw |= model.apply_git_activity(update);
        }
        while let Some(result) = runtime.focus.try_recv() {
            if !flows.actions.is_active(result.action_id)
                || !flows.task.as_ref().is_some_and(
                    |task| matches!(task, action::Task::Focus { id } if *id == result.action_id),
                )
            {
                continue;
            }
            let focused_world = model
                .pane_route(&result.target)
                .map(|(index, _)| index)
                .filter(|index| sessions.control_path(*index) == result.control_path);
            let succeeded =
                result.result.is_ok() && focused_world.is_some_and(|index| sessions.is_open(index));
            if succeeded {
                model.open_world(focused_world.expect("successful focus has a world"));
            } else {
                flows.action_error = Some(
                    result
                        .result
                        .err()
                        .unwrap_or_else(|| "Codex pane is no longer openable".into()),
                );
            }
            flows.actions.acknowledge(result.action_id, succeeded);
            flows.task = None;
            redraw = true;
        }
        redraw |= action::poll(
            &mut flows,
            sessions,
            model,
            runtime.refresh,
            terminal
                .size()
                .context("read wt shell terminal area")?
                .into(),
        )?;
        redraw |= action::start_next(&mut flows, sessions, model, runtime, area);
        redraw |= action::apply_removed(&mut flows.actions, model);
        if redraw {
            let screens = sessions.screens();
            let closed_message = model
                .has_worlds()
                .then(|| sessions.closed_message(model.active()))
                .flatten();
            terminal.draw(|frame| {
                render::draw(
                    frame,
                    &screens,
                    closed_message,
                    model,
                    flows.creation.as_ref(),
                    flows.action_error.as_deref(),
                    flows.deletion.as_ref(),
                );
                if flows.queue_panel_visible() {
                    flows
                        .actions
                        .render(frame, frame.area(), flows.queue_panel_compact());
                }
                if let Some(task) = &flows.task {
                    task.render_overlay(frame);
                }
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
                runtime,
                &mut flows,
            )?;
            if model.should_quit() {
                return Ok(flows.actions.running_work());
            }
            if !event::poll(Duration::ZERO).context("poll pending terminal input")? {
                break;
            }
        }
    }
    Ok(flows.actions.running_work())
}

fn dispatch_event(
    event: Event,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    area: Rect,
    runtime: &ShellRuntime<'_>,
    flows: &mut ControlFlows,
) -> Result<bool> {
    if request_close(&event, model, area) {
        return Ok(true);
    }
    let queue_panel_compact = flows.queue_panel_compact();
    if flows.queue_panel_visible()
        && flows
            .actions
            .handle_mouse(&event, area, queue_panel_compact)
    {
        let _ = action::apply_removed(&mut flows.actions, model);
        return Ok(true);
    }
    if let Some(flow) = flows.creation.as_mut() {
        if flow.handle_progress_mouse(&event, area) {
            return Ok(true);
        }
        if flow.blocks_input() {
            if let Event::Mouse(mouse) = event {
                let action = flow.handle_mouse(mouse, area);
                let _ = apply_creation_action(action, flows)?;
                return Ok(true);
            }
        }
    }
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if model.mode() != Mode::Control
                && sessions.closed_message(model.active()).is_some()
                && key.code == crossterm::event::KeyCode::Char(' ')
            {
                let identity = model.worlds()[model.active()].identity.clone();
                flows
                    .actions
                    .enqueue(action_queue::Intent::Reconnect(identity));
                return Ok(true);
            }
            let active_creation_action = flows
                .task
                .as_mut()
                .and_then(action::Task::blocking_create_mut)
                .map(|(id, flow)| (id, flow.handle_key(key, runtime.config)));
            if let Some((id, action)) = active_creation_action {
                if matches!(action, crate::create::FlowAction::Cancelling) {
                    flows.actions.begin_cancellation(id);
                    let _ = action::apply_removed(&mut flows.actions, model);
                }
                return Ok(true);
            }
            if let Some(flow) = flows.creation.as_mut().filter(|flow| flow.blocks_input()) {
                let action = flow.handle_key(key, runtime.config);
                let _ = apply_creation_action(action, flows)?;
                return Ok(true);
            }
            if key.code == crossterm::event::KeyCode::F(5) {
                if model.handle_key(key, area) == InputRoute::World {
                    if sessions.closed_message(model.active()).is_some() {
                        return Ok(true);
                    }
                    let screen = sessions.screen(model.active());
                    if let Some(bytes) = input::encode_key(key, screen.application_cursor())? {
                        sessions.write(model.active(), &bytes)?;
                    }
                }
                return Ok(true);
            }
            if flows.action_error.is_some()
                && matches!(
                    key.code,
                    crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Esc
                )
            {
                flows.action_error.take();
                return Ok(true);
            }
            if let Some(flow) = flows.deletion.as_mut() {
                let action = flow.handle_event(&Event::Key(key), area, runtime.config);
                let _ = apply_deletion_action(action, flows)?;
                return Ok(true);
            }
            match model.handle_key(key, area) {
                InputRoute::World => {
                    if sessions.closed_message(model.active()).is_some() {
                        return Ok(true);
                    }
                    let screen = sessions.screen(model.active());
                    if let Some(bytes) = input::encode_key(key, screen.application_cursor())? {
                        sessions.write(model.active(), &bytes)?;
                    }
                }
                InputRoute::Command(command) => {
                    start_control_command(command, runtime, model, flows);
                }
                InputRoute::OpenPane(identity) => {
                    flows
                        .actions
                        .enqueue(action_queue::Intent::OpenPane(*identity));
                }
                InputRoute::DeleteWorld(world) => {
                    flows.deletion = Some(delete::Flow::confirm(*world));
                }
                InputRoute::Consumed => {}
            }
            Ok(true)
        }
        Event::Paste(text) if flows.creation.as_ref().is_some() => {
            if let Some(flow) = flows.creation.as_mut() {
                let _ = flow.handle_paste(&text);
            }
            Ok(true)
        }
        Event::Paste(text) if flows.deletion.as_ref().is_some() => {
            if let Some(flow) = flows.deletion.as_mut() {
                let _ = flow.handle_paste(&text);
            }
            Ok(true)
        }
        Event::Paste(_) if model.control().palette().is_open() => Ok(true),
        Event::Paste(text) if model.mode() == Mode::World => {
            if sessions.closed_message(model.active()).is_some() {
                return Ok(true);
            }
            let bracketed = sessions.screen(model.active()).bracketed_paste();
            sessions.write(model.active(), &input::encode_paste(&text, bracketed))?;
            Ok(true)
        }
        Event::Mouse(mouse) if flows.deletion.as_ref().is_some() => {
            if let Some(flow) = flows.deletion.as_mut() {
                let action = flow.handle_event(&Event::Mouse(mouse), area, runtime.config);
                let _ = apply_deletion_action(action, flows)?;
            }
            Ok(true)
        }
        Event::Mouse(mouse)
            if model.mode().forwards_mouse()
                && !model.control().palette().is_open()
                && mouse.row == area.y =>
        {
            let (changed, _) = model.handle_mouse(mouse, area);
            Ok(changed)
        }
        Event::Mouse(mouse)
            if model.mode().forwards_mouse() && !model.control().palette().is_open() =>
        {
            if sessions.closed_message(model.active()).is_some() {
                return Ok(false);
            }
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
        Event::Mouse(mouse)
            if model.mode() == Mode::Control || model.control().palette().is_open() =>
        {
            if let Some(flow) = flows.deletion.as_mut() {
                let action = flow.handle_event(&Event::Mouse(mouse), area, runtime.config);
                let _ = apply_deletion_action(action, flows)?;
            } else {
                let world_card_count = model.world_count()
                    + usize::from(
                        flows
                            .creation
                            .as_ref()
                            .and_then(crate::create::Flow::creating_world)
                            .is_some_and(|(name, _)| {
                                model.worlds().iter().all(|world| world.name != name)
                            }),
                    );
                let (changed, route) =
                    model.handle_mouse_with_world_count(mouse, area, world_card_count);
                match route {
                    Some(InputRoute::Command(command)) => {
                        start_control_command(command, runtime, model, flows)
                    }
                    Some(InputRoute::OpenPane(identity)) => {
                        flows
                            .actions
                            .enqueue(action_queue::Intent::OpenPane(*identity));
                    }
                    Some(InputRoute::DeleteWorld(world)) => {
                        flows.deletion = Some(delete::Flow::confirm(*world));
                    }
                    Some(InputRoute::Consumed | InputRoute::World) | None => {}
                }
                return Ok(changed);
            }
            Ok(true)
        }
        Event::Resize(columns, rows) => {
            let area = Rect::new(0, 0, columns, rows);
            model.resize(area);
            let (rows, columns) = session_viewport(model, area);
            sessions.resize(model.mode(), model.active(), rows, columns)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn world_rows(terminal_rows: u16) -> u16 {
    terminal_rows.saturating_sub(BAR_HEIGHT).max(1)
}

fn session_viewport(_model: &ShellModel, area: Rect) -> (u16, u16) {
    (world_rows(area.height), area.width)
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
    git_author: &Result<crate::git_author::GitAuthor, String>,
    model: &ShellModel,
    flows: &mut ControlFlows,
) {
    match command {
        ControlCommand::NewWorld => {}
        ControlCommand::DeleteWorld => unreachable!("delete is handled separately"),
    }
    let mut used_names = model
        .worlds()
        .iter()
        .map(|world| world.world_name.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    used_names.extend(flows.actions.create_names().map(str::to_owned));
    let author = match git_author {
        Ok(author) => author.clone(),
        Err(error) => {
            flows.action_error = Some(error.clone());
            return;
        }
    };
    match crate::create::prepare_with_author(config, author, &used_names) {
        Ok(flow) => {
            flows.creation = Some(flow);
            flows.action_error = None;
        }
        Err(cause) => flows.action_error = Some(format!("{cause:#}")),
    }
}

fn apply_creation_action(
    action: crate::create::FlowAction,
    flows: &mut ControlFlows,
) -> Result<bool> {
    match action {
        crate::create::FlowAction::None => Ok(false),
        crate::create::FlowAction::Changed => Ok(true),
        crate::create::FlowAction::Submit(input) => {
            flows.actions.enqueue(action_queue::Intent::Create(input));
            flows.creation.take();
            Ok(true)
        }
        crate::create::FlowAction::Cancel => {
            flows.creation.take();
            Ok(true)
        }
        crate::create::FlowAction::Cancelling => {
            unreachable!("a creation form cannot be cancelling")
        }
        crate::create::FlowAction::Failed(message) => {
            flows.creation.take();
            flows.action_error = Some(message);
            Ok(true)
        }
        crate::create::FlowAction::Created(_) => unreachable!("a form cannot create a world"),
    }
}

fn apply_deletion_action(action: delete::FlowAction, flows: &mut ControlFlows) -> Result<bool> {
    match action {
        delete::FlowAction::None => Ok(false),
        delete::FlowAction::Changed => Ok(true),
        delete::FlowAction::Submit(world) => {
            flows.actions.enqueue(action_queue::Intent::Delete(*world));
            flows.deletion.take();
            Ok(true)
        }
        delete::FlowAction::Cancel => {
            flows.deletion.take();
            Ok(true)
        }
    }
}

impl ControlFlows {
    fn queue_panel_visible(&self) -> bool {
        !self.task.as_ref().is_some_and(action::Task::blocks_input)
    }

    fn queue_panel_compact(&self) -> bool {
        self.creation.is_some() || self.deletion.is_some()
    }
}

#[cfg(test)]
mod tests;
