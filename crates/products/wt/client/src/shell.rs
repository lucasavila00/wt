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

mod activity;
mod bar;
mod codex;
mod control;
mod delete;
mod input;
mod lifecycle;
mod live;
mod live_focus;
mod model;
mod refresh;
mod refresh_status;
mod render;
mod scrollbar;
mod session;
mod terminal_view;
mod toast;
mod world_card;
use control::ControlCommand;
use lifecycle::start_control_command;
use model::{InputRoute, Mode, ShellModel, ShellWorld};
use refresh::{take_current_snapshot, CodexRefresh, WorldRefresh};
use session::SessionSet;

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
    ssh::sync(config, &report.instances)?;
    let worlds = shell_worlds(&report.instances);
    let (columns, rows) = crossterm::terminal::size().context("read terminal size")?;
    let mut sessions = SessionSet::start(&worlds, world_rows(rows), columns)?;
    let mut model = ShellModel::new(worlds);
    model.control_mut().set_capacity(report.capacity);
    model.set_test_server(test_server);
    model.finish_worlds_refresh(Ok(refresh::updated_at()));
    let focus = codex::FocusWorker::default();
    let mut live_focus = live_focus::LiveFocus::default();
    let refresh = WorldRefresh::start(config.clone());
    let codex_refresh = CodexRefresh::start(config.clone());
    let runtime = ShellRuntime {
        config,
        refresh: &refresh,
        codex_refresh: &codex_refresh,
        focus: &focus,
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
        &mut live_focus,
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
    result.and(input_result)
}

fn shell_worlds(instances: &[inventory::ContextInstance]) -> Vec<ShellWorld> {
    instances
        .iter()
        .filter(|world| ssh::has_alias(world))
        .map(codex::ShellWorld::from_inventory)
        .collect()
}

struct ShellRuntime<'a> {
    config: &'a ClientConfig,
    refresh: &'a WorldRefresh,
    codex_refresh: &'a CodexRefresh,
    focus: &'a codex::FocusWorker,
}

#[derive(Default)]
struct ControlFlows {
    creation: Option<crate::create::Flow>,
    creation_error: Option<String>,
    deletion: Option<delete::Flow>,
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
    live_focus: &mut live_focus::LiveFocus,
    runtime: &ShellRuntime<'_>,
    shutdown: &AtomicBool,
) -> Result<()> {
    let mut redraw = true;
    let mut flows = ControlFlows::default();
    while !shutdown.load(Ordering::Relaxed) {
        let area: Rect = terminal
            .size()
            .context("read wt shell terminal area")?
            .into();
        let (rows, columns) = session_viewport(model, area);
        sessions.resize(rows, columns)?;
        let (output_changed, clipboard_writes) = sessions.drain_output(model.active());
        redraw |= output_changed;
        if model.mode() == Mode::Control && model.control().activity() == control::Activity::Live {
            live_focus.sync(model, sessions, runtime.focus);
        } else {
            live_focus.clear();
        }
        for sequence in clipboard_writes {
            terminal
                .backend_mut()
                .write_all(&sequence)
                .context("relay world clipboard write")?;
        }
        if flows.deletion.is_none() {
            if let Some(snapshot) = take_current_snapshot(
                &runtime.refresh.updates,
                runtime.refresh.generation.load(Ordering::Relaxed),
            ) {
                if !snapshot.failures.is_empty() {
                    model.finish_worlds_refresh(Err(snapshot.failures));
                    redraw = true;
                } else if ssh::sync(runtime.config, &snapshot.instances).is_ok() {
                    let worlds = shell_worlds(&snapshot.instances);
                    let area: Rect = terminal
                        .size()
                        .context("read wt shell terminal area")?
                        .into();
                    sessions.reconcile(&worlds, world_rows(area.height), area.width)?;
                    model.reconcile_worlds(worlds);
                    model.control_mut().set_capacity(snapshot.capacity);
                    model.finish_worlds_refresh(Ok(refresh::updated_at()));
                    redraw = true;
                }
            }
        }
        if let Some(snapshot) = runtime.codex_refresh.updates.try_iter().last() {
            let live_worlds = model
                .worlds()
                .iter()
                .enumerate()
                .filter(|(index, _)| sessions.is_open(*index))
                .map(|(_, world)| world.clone())
                .collect::<Vec<_>>();
            let area = terminal
                .size()
                .context("read wt shell terminal area")?
                .into();
            let snapshot = codex::cards(snapshot, &live_worlds);
            redraw |= model.control_mut().apply_codex_refresh(
                snapshot.cards,
                snapshot.failures,
                refresh::updated_at(),
                area,
            );
        }
        while let Some(result) = runtime.focus.try_recv() {
            redraw = true;
            let focused_world = model
                .focus_route(&result.target)
                .map(|(index, _)| index)
                .filter(|index| sessions.control_path(*index) == result.control_path);
            if !result.open_world {
                if focused_world.is_some() {
                    live_focus.complete(&result.target, result.result.is_ok());
                }
                continue;
            }
            match result.result {
                Ok(()) => match focused_world {
                    Some(index) if sessions.is_open(index) => {
                        model.finish_codex_open(&result.target, Some(index), false)
                    }
                    Some(_) | None => model.finish_codex_open(&result.target, None, true),
                },
                Err(_) => model.finish_codex_open(&result.target, None, true),
            }
        }
        if let Some(action) = flows.creation.as_mut().map(crate::create::Flow::poll) {
            redraw |= apply_creation_action(
                action,
                &mut flows.creation,
                &mut flows.creation_error,
                sessions,
                model,
                runtime.refresh,
                terminal
                    .size()
                    .context("read wt shell terminal area")?
                    .into(),
            )?;
        }
        redraw |= flows
            .creation
            .as_ref()
            .is_some_and(|flow| !flow.blocks_input());
        if let Some(action) = flows.deletion.as_mut().map(delete::Flow::poll) {
            redraw |= apply_deletion_action(
                action,
                &mut flows.deletion,
                sessions,
                model,
                runtime.refresh,
                terminal
                    .size()
                    .context("read wt shell terminal area")?
                    .into(),
            )?;
        }
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
                    live_focus,
                    closed_message,
                    model,
                    flows.creation.as_ref(),
                    flows.creation_error.as_deref(),
                    flows.deletion.as_ref(),
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
                runtime,
                &mut flows,
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
    flows: &mut ControlFlows,
) -> Result<bool> {
    if let Some(flow) = flows.creation.as_mut() {
        if flow.handle_progress_mouse(&event, area) {
            return Ok(true);
        }
        if flow.blocks_input() {
            if let Event::Mouse(mouse) = event {
                let action = flow.handle_mouse(mouse, area);
                let _ = apply_creation_action(
                    action,
                    &mut flows.creation,
                    &mut flows.creation_error,
                    sessions,
                    model,
                    runtime.refresh,
                    area,
                )?;
                return Ok(true);
            }
        }
    }
    if let Some(flow) = flows.deletion.as_mut() {
        if flow.handle_progress_mouse(&event, area) {
            return Ok(true);
        }
    }
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if model.mode() != Mode::Control
                && sessions.closed_message(model.active()).is_some()
                && key.code == crossterm::event::KeyCode::Char(' ')
            {
                sessions.restart(model.active(), world_rows(area.height), area.width);
                return Ok(true);
            }
            if let Some(flow) = flows.creation.as_mut().filter(|flow| flow.blocks_input()) {
                let action = flow.handle_key(key, runtime.config);
                let _ = apply_creation_action(
                    action,
                    &mut flows.creation,
                    &mut flows.creation_error,
                    sessions,
                    model,
                    runtime.refresh,
                    area,
                )?;
                return Ok(true);
            }
            if matches!(key.code, crossterm::event::KeyCode::F(5 | 6)) {
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
            if flows.creation_error.is_some()
                && matches!(
                    key.code,
                    crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Esc
                )
            {
                flows.creation_error.take();
                return Ok(true);
            }
            if let Some(flow) = flows.deletion.as_mut().filter(|flow| flow.blocks_input()) {
                let action = flow.handle_event(&Event::Key(key), area, runtime.config);
                let _ = apply_deletion_action(
                    action,
                    &mut flows.deletion,
                    sessions,
                    model,
                    runtime.refresh,
                    area,
                )?;
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
                    start_control_command(command, runtime.config, runtime.refresh, model, flows);
                }
                InputRoute::OpenCodex(target) => {
                    runtime.focus.start_for_session(sessions, model, *target)
                }
                InputRoute::Consumed => {}
            }
            Ok(true)
        }
        Event::Paste(text)
            if flows
                .creation
                .as_ref()
                .is_some_and(|flow| flow.blocks_input()) =>
        {
            if let Some(flow) = flows.creation.as_mut() {
                let _ = flow.handle_paste(&text);
            }
            Ok(true)
        }
        Event::Paste(text)
            if flows
                .deletion
                .as_ref()
                .is_some_and(|flow| flow.blocks_input()) =>
        {
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
        Event::Mouse(mouse)
            if flows
                .deletion
                .as_ref()
                .is_some_and(|flow| flow.blocks_input()) =>
        {
            if let Some(flow) = flows.deletion.as_mut().filter(|flow| flow.blocks_input()) {
                let action = flow.handle_event(&Event::Mouse(mouse), area, runtime.config);
                let _ = apply_deletion_action(
                    action,
                    &mut flows.deletion,
                    sessions,
                    model,
                    runtime.refresh,
                    area,
                )?;
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
                let _ = apply_deletion_action(
                    action,
                    &mut flows.deletion,
                    sessions,
                    model,
                    runtime.refresh,
                    area,
                )?;
            } else {
                let (changed, route) = model.handle_mouse(mouse, area);
                match route {
                    Some(InputRoute::Command(command)) => start_control_command(
                        command,
                        runtime.config,
                        runtime.refresh,
                        model,
                        flows,
                    ),
                    Some(InputRoute::OpenCodex(target)) => {
                        runtime.focus.start_for_session(sessions, model, *target)
                    }
                    Some(InputRoute::Consumed | InputRoute::World) | None => {}
                }
                return Ok(changed);
            }
            Ok(true)
        }
        Event::Resize(columns, rows) => {
            sessions.resize(world_rows(rows), columns)?;
            model.resize(Rect::new(0, 0, columns, rows));
            Ok(true)
        }
        _ => Ok(false),
    }
}
fn world_rows(terminal_rows: u16) -> u16 {
    terminal_rows.saturating_sub(BAR_HEIGHT).max(1)
}

fn session_viewport(model: &ShellModel, area: Rect) -> (u16, u16) {
    if model.mode() == Mode::Control && model.control().activity() == control::Activity::Live {
        live::preview_size(area, model.control().live_codex().len())
    } else {
        (world_rows(area.height), area.width)
    }
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
    model: &ShellModel,
    creation: &mut Option<crate::create::Flow>,
    error: &mut Option<String>,
) {
    match command {
        ControlCommand::NewWorld => {}
        ControlCommand::DeleteWorld => unreachable!("delete is handled separately"),
    }
    let used_names = model
        .worlds()
        .iter()
        .map(|world| world.instance_name.to_string())
        .collect();
    match crate::create::prepare(config, &used_names) {
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
        crate::create::FlowAction::Changed => {
            if creation
                .as_ref()
                .and_then(crate::create::Flow::creating_world)
                .is_some()
            {
                model.show_worlds();
            }
            Ok(true)
        }
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
            let world = codex::ShellWorld::from_instance(&created.context, &created.instance);
            refresh.invalidate();
            if model.world_index(&world.identity).is_none() {
                sessions.add_world(&world, world_rows(area.height), area.width)?;
                let mut worlds = model.worlds().to_vec();
                worlds.push(world);
                model.reconcile_worlds(worlds);
            }
            creation.take();
            Ok(true)
        }
    }
}

fn apply_deletion_action(
    action: delete::FlowAction,
    deletion: &mut Option<delete::Flow>,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    refresh: &WorldRefresh,
    area: Rect,
) -> Result<bool> {
    match action {
        delete::FlowAction::None => Ok(false),
        delete::FlowAction::Changed => Ok(true),
        delete::FlowAction::Cancel => {
            deletion.take();
            Ok(true)
        }
        delete::FlowAction::Deleted(identity) => {
            refresh.invalidate();
            let worlds = model
                .worlds()
                .iter()
                .filter(|world| world.identity != identity)
                .cloned()
                .collect::<Vec<_>>();
            sessions.reconcile(&worlds, world_rows(area.height), area.width)?;
            model.reconcile_worlds(worlds);
            deletion.take();
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests;
