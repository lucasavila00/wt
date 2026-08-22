use super::{Key, Screen};
use anyhow::Result;

pub(crate) fn create_world_with_defaults(screen: &mut Screen, name: &str) -> Result<()> {
    open_command(screen, "new", "Create world")?;
    screen
        .press(Key::Enter)?
        .type_text(name)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("Review")?
        .press(Key::Enter)?
        .wait_for_text(name)?
        .wait_for_text("RUNNING")?;
    Ok(())
}

pub(crate) fn delete_world(screen: &mut Screen, name: &str) -> Result<()> {
    open_command(screen, "delete", "Delete world")?;
    screen
        .type_text(name)?
        .press(Key::Enter)?
        .wait_for_text("Delete world?")?
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text(&format!("Deleting {name}…"))?
        .wait_for_text_gone(&format!("Deleting {name}…"))?;
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
