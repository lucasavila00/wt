use crate::WorldName;
use serde::{Deserialize, Serialize};
use wt_world::WorldId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaneObservation {
    pub world_id: WorldId,
    pub world_name: WorldName,
    pub tmux_session: String,
    pub pane_id: String,
    pub changed_at_unix_ms: i64,
    pub observed_at_unix_ms: i64,
}
