pub(super) use super::activity::Activity;

mod command;
mod layout;
mod live;
mod palette;
mod state;
mod types;

pub(super) use command::command_palette_layout;
pub(super) use layout::{
    codex_card_rects, control_areas, control_content_areas, world_card_at_position,
    world_card_rects, ACTIVITY_BUTTON_HEIGHT,
};
pub(super) use palette::CommandPalette;
pub(super) use state::ControlState;
pub(super) use types::{
    CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget, ControlAction, ControlCommand,
};

#[cfg(test)]
#[path = "control/tests.rs"]
mod tests;
