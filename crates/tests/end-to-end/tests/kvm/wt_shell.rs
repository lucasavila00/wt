use super::{Key, Screen};
use anyhow::{bail, Result};
use std::time::{Duration, Instant};

pub(crate) fn create_world_with_defaults(screen: &mut Screen, name: &str) -> Result<()> {
    eprintln!("WT shell E2E: open the new-world form");
    open_command(screen, "new", "Create world")?;
    eprintln!("WT shell E2E: submit world {name}");
    screen
        .press(Key::Enter)?
        .type_text(name)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("Review")?
        .press(Key::Enter)?
        .wait_for_text(name)?;
    wait_for_slow_text(screen, "RUNNING", Duration::from_secs(90))?;
    screen.wait_for_text_gone("World creation")?;
    eprintln!("WT shell E2E: open world {name}");
    screen.press(Key::Enter)?;
    wait_for_slow_text(screen, "wt@ubuntu:", Duration::from_secs(30))?;
    eprintln!("WT shell E2E: world {name} is running and open");
    Ok(())
}

pub(crate) fn delete_world(screen: &mut Screen, name: &str) -> Result<()> {
    eprintln!("WT shell E2E: open control mode before deleting {name}");
    screen.press(Key::Up)?.wait_for_text("Worlds ·")?;
    eprintln!("WT shell E2E: delete world {name}");
    open_command(screen, "delete", "Delete world")?;
    screen
        .type_text(name)?
        .press(Key::Enter)?
        .wait_for_text("Delete world?")?
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text(&format!("Deleting {name}…"))?
        .wait_for_text_gone(&format!("Deleting {name}…"))?;
    eprintln!("WT shell E2E: world {name} was deleted");
    Ok(())
}

fn open_command(screen: &mut Screen, query: &str, expected: &str) -> Result<()> {
    screen
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .type_text(query)?
        .press(Key::Enter)?
        .wait_for_text(expected)?;
    Ok(())
}

fn wait_for_slow_text(screen: &mut Screen, expected: &str, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        match screen.wait_for_text(expected) {
            Ok(_) => return Ok(()),
            Err(error) if started.elapsed() < timeout => {
                eprintln!(
                    "WT shell E2E: still waiting for {expected:?} after {:.0}s\n{error:#}",
                    started.elapsed().as_secs_f64()
                );
            }
            Err(error) => {
                bail!("WT shell E2E: {expected:?} did not appear within {timeout:?}: {error:#}")
            }
        }
    }
}
