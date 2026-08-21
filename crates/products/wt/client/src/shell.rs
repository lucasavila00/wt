use anyhow::{bail, Context as _, Result};
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind};
use crossterm::execute;
use std::io::IsTerminal as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wt_client::config::ClientConfig;
use wt_client::{inventory, ssh};

mod input;
mod model;
mod render;
mod session;

use model::{InputRoute, Mode, ShellModel};
use session::SessionSet;

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
    let mut sessions = SessionSet::start(&worlds, rows, columns)?;
    let mut model = ShellModel::new(worlds);
    let shutdown = install_signal_handlers()?;
    let mut terminal = ratatui::init();
    if let Err(error) = execute!(terminal.backend_mut(), EnableBracketedPaste) {
        ratatui::restore();
        return Err(error).context("enable bracketed paste for wt shell");
    }

    let result = run_loop(&mut terminal, &mut sessions, &mut model, &shutdown);
    let paste_result = execute!(terminal.backend_mut(), DisableBracketedPaste)
        .context("disable bracketed paste for wt shell");
    ratatui::restore();
    result.and(paste_result)
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
        redraw |= sessions.drain_output();
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
            redraw |= dispatch_event(
                event::read().context("read terminal input")?,
                sessions,
                model,
            )?;
            if !event::poll(Duration::ZERO).context("poll pending terminal input")? {
                break;
            }
        }
    }
    Ok(())
}

fn dispatch_event(event: Event, sessions: &mut SessionSet, model: &mut ShellModel) -> Result<bool> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if model.handle_key(key.code) == InputRoute::World {
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
        Event::Resize(columns, rows) => {
            sessions.resize(rows, columns)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}
