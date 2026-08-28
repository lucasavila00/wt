use crate::WorldName;
use serde::{Deserialize, Serialize};
use wt_world::WorldId;

pub const MAX_PANE_FRAME_ROWS: u16 = 100;
pub const MAX_PANE_FRAME_COLUMNS: u16 = 300;
pub const MAX_PANE_FRAME_CELLS: usize = 30_000;
pub const MAX_PANE_CELL_TEXT_BYTES: usize = 32;
pub const MAX_PANE_WINDOW_NAME_BYTES: usize = 255;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaneObservation {
    pub world_id: WorldId,
    pub world_name: WorldName,
    #[serde(default)]
    pub created_at_unix_ms: i64,
    pub tmux_session: String,
    pub pane_id: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub changed_at_unix_ms: i64,
    pub observed_at_unix_ms: i64,
    pub render: PaneRender,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaneRender {
    pub window_index: i64,
    pub window_name: String,
    pub frame: PaneFrame,
}

impl PaneRender {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.window_index < 0 {
            return Err("window index is negative");
        }
        if self.window_name.is_empty()
            || self.window_name.len() > MAX_PANE_WINDOW_NAME_BYTES
            || self.window_name.chars().any(char::is_control)
        {
            return Err("window name is invalid display text");
        }
        self.frame.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaneFrame {
    pub rows: u16,
    pub columns: u16,
    pub cells: Vec<PaneCell>,
}

impl PaneFrame {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.rows == 0
            || self.columns == 0
            || self.rows > MAX_PANE_FRAME_ROWS
            || self.columns > MAX_PANE_FRAME_COLUMNS
        {
            return Err("frame dimensions are out of bounds");
        }
        let expected_cells = usize::from(self.rows) * usize::from(self.columns);
        if expected_cells > MAX_PANE_FRAME_CELLS || self.cells.len() != expected_cells {
            return Err("frame cells do not match dimensions");
        }
        if self.cells.iter().any(|cell| !cell.valid()) {
            return Err("frame contains an invalid cell");
        }
        Ok(())
    }

    pub fn cell(&self, row: u16, column: u16) -> Option<&PaneCell> {
        if row >= self.rows || column >= self.columns {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.columns) + usize::from(column))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaneCell {
    pub text: String,
    #[serde(default, skip_serializing_if = "PaneColor::is_default")]
    pub foreground: PaneColor,
    #[serde(default, skip_serializing_if = "PaneColor::is_default")]
    pub background: PaneColor,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underlined: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inverse: bool,
}

impl PaneCell {
    fn valid(&self) -> bool {
        !self.text.is_empty()
            && self.text.len() <= MAX_PANE_CELL_TEXT_BYTES
            && !self.text.chars().any(char::is_control)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaneColor {
    #[default]
    Default,
    Indexed {
        index: u8,
    },
    Rgb {
        red: u8,
        green: u8,
        blue: u8,
    },
}

impl PaneColor {
    fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(text: &str) -> PaneCell {
        PaneCell {
            text: text.into(),
            foreground: PaneColor::Default,
            background: PaneColor::Default,
            bold: false,
            italic: false,
            underlined: false,
            inverse: false,
        }
    }

    #[test]
    fn pane_frames_require_a_complete_inert_cell_grid() {
        let valid = PaneFrame {
            rows: 1,
            columns: 2,
            cells: vec![cell("a"), cell(" ")],
        };
        assert_eq!(valid.validate(), Ok(()));
        assert_eq!(valid.cell(1, 0), None);

        let incomplete = PaneFrame {
            cells: vec![cell("a")],
            ..valid.clone()
        };
        assert_eq!(
            incomplete.validate(),
            Err("frame cells do not match dimensions")
        );

        let control = PaneFrame {
            cells: vec![cell("\u{1b}"), cell(" ")],
            ..valid
        };
        assert_eq!(control.validate(), Err("frame contains an invalid cell"));
    }

    #[test]
    fn pane_render_is_bounded_presentation_data() {
        let mut render = PaneRender {
            window_index: 0,
            window_name: "codex".into(),
            frame: PaneFrame {
                rows: 1,
                columns: 1,
                cells: vec![cell("C")],
            },
        };
        assert_eq!(render.validate(), Ok(()));

        render.window_index = -1;
        assert_eq!(render.validate(), Err("window index is negative"));
        render.window_index = 0;
        render.window_name = "\n".into();
        assert_eq!(
            render.validate(),
            Err("window name is invalid display text")
        );
    }
}
