pub(super) use super::activity::Activity;

mod command;
mod help;
mod layout;
mod live;
mod palette;
mod state;
mod types;

pub(super) use command::command_palette_layout;
pub(super) use help::{help_control_area, Help, HELP_CONTROL};
pub(super) use layout::{
    card_grid, card_grid_with_gap, codex_card_grid, control_areas, control_content_areas,
    world_card_action_at_position, world_card_at_position, CardGrid, ACTIVITY_BUTTON_HEIGHT,
    CARD_COLUMNS, WORLD_CARD_HEIGHT,
};
pub(super) use palette::CommandPalette;
pub(super) use state::ControlState;
pub(super) use types::{
    CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget, ControlAction, ControlCommand,
};

#[cfg(test)]
#[path = "control/tests.rs"]
mod tests;
