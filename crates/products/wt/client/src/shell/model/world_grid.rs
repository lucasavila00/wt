use super::{KeyCode, ShellModel};

impl ShellModel {
    pub(super) fn move_world_grid_selection(&mut self, code: KeyCode) -> bool {
        let delta = match code {
            KeyCode::Up => -2,
            KeyCode::Down => 2,
            KeyCode::Left => -1,
            KeyCode::Right => 1,
            _ => return false,
        };
        self.active = self
            .active
            .saturating_add_signed(delta)
            .min(self.worlds.len() - 1);
        true
    }
}
