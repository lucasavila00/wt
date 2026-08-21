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

mod control;
mod input;
mod model;
mod render;
mod session;

use control::ControlCommand;
use model::{InputRoute, Mode, ShellModel};
use session::SessionSet;

const BAR_HEIGHT: u16 = 1;

pub fn run(config: &ClientConfig) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("wt shell requires an interactive terminal");
    }
    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        return Err(crate::context_failures(
            "wt shell was not started because the complete world list is unavailable",
            &report.failures,
            None,
        ));
    }
    ssh::sync(config, &report.instances)?;
    let worlds = report
        .instances
        .iter()
        .filter(|world| ssh::has_alias(world))
        .map(inventory::ContextInstance::qualified_name)
        .collect::<Vec<_>>();
    if worlds.is_empty() {
        bail!("wt shell found no worlds with SSH access");
    }

    let (columns, rows) = crossterm::terminal::size().context("read terminal size")?;
    let mut sessions = SessionSet::start(&worlds, world_rows(rows), columns)?;
    let mut model = ShellModel::new(worlds);
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

    let result = run_loop(&mut terminal, &mut sessions, &mut model, config, &shutdown);
    let input_result = execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("disable terminal input for wt shell");
    ratatui::restore();
    result.and(input_result)
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
    shutdown: &AtomicBool,
) -> Result<()> {
    let mut redraw = true;
    let mut creation = None;
    let mut creation_error = None;
    while !shutdown.load(Ordering::Relaxed) {
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
                terminal
                    .size()
                    .context("read wt shell terminal area")?
                    .into(),
            )?;
        }
        if redraw {
            let screen = sessions.screen(model.active());
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
        if sessions.all_closed() {
            return Ok(());
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
                config,
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
    config: &ClientConfig,
    creation: &mut Option<crate::create::Flow>,
    creation_error: &mut Option<String>,
) -> Result<bool> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if matches!(key.code, crossterm::event::KeyCode::F(5 | 6)) {
                let _ = model.handle_key(key);
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
                    let action = flow.handle_key(key, config);
                    let _ = apply_creation_action(
                        action,
                        creation,
                        creation_error,
                        sessions,
                        model,
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
                    start_creation(command, config, creation, creation_error);
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
                    start_creation(command, config, creation, creation_error);
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
    creation: &mut Option<crate::create::Flow>,
    error: &mut Option<String>,
) {
    let kind = match command {
        ControlCommand::NewDev => Ok(crate::create::Kind::Dev),
        ControlCommand::NewHost => crate::host::default_input().map(crate::create::Kind::Host),
    };
    match kind.and_then(|kind| crate::create::prepare(config, kind)) {
        Ok(flow) => {
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
            let world = format!("{}.{}", created.context, created.instance.name);
            if model.world_index(&world).is_none() {
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
}
