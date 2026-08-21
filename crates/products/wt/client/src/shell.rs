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

use model::{InputRoute, Mode, ShellModel};
use session::SessionSet;
use wt_control_protocol::{ApiRequest, Operation, Response};

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
    model.set_codex(load_codex(config));
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

    let result = run_loop(&mut terminal, &mut sessions, &mut model, &shutdown);
    let input_result = execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("disable terminal input for wt shell");
    ratatui::restore();
    result.and(input_result)
}

fn load_codex(config: &ClientConfig) -> Vec<control::CodexContextSnapshot> {
    let request = ApiRequest::new(Operation::ListCodexSessions);
    config
        .contexts
        .iter()
        .map(
            |context| match wt_client::transport::call(context, &request) {
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
            },
        )
        .collect()
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
    shutdown: &AtomicBool,
) -> Result<()> {
    let mut redraw = true;
    while !shutdown.load(Ordering::Relaxed) {
        let (output_changed, clipboard_writes) = sessions.drain_output(model.active());
        redraw |= output_changed;
        for sequence in clipboard_writes {
            terminal
                .backend_mut()
                .write_all(&sequence)
                .context("relay world clipboard write")?;
        }
        if redraw {
            let screen = sessions.screen(model.active());
            terminal.draw(|frame| render::draw(frame, screen, model))?;
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
) -> Result<bool> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if model.handle_key(key) == InputRoute::World {
                let screen = sessions.screen(model.active());
                if let Some(bytes) = input::encode_key(key, screen.application_cursor())? {
                    sessions.write(model.active(), &bytes)?;
                }
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
        Event::Mouse(mouse) if model.mode() == Mode::Control => Ok(model.handle_mouse(mouse, area)),
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
