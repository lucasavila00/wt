use crate::WorldName;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
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

// Client card rendering retains these private compatibility shapes while it is
// converted from session cards to pane cards. They are not server state or API
// responses.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSessionState {
    Unknown,
    Working,
    NeedsAttention,
    Inactive,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ByobuTarget {
    pub tmux_session: String,
    pub pane_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodexSessionObservation {
    pub world_id: WorldId,
    pub world_name: WorldName,
    pub cwd: String,
    pub repository_root: Option<String>,
    pub repository_url: Option<String>,
    pub git_branch: Option<String>,
    pub git_context_checked_at_unix_ms: Option<i64>,
    pub git_context_error: Option<String>,
    pub state: CodexSessionState,
    pub is_compacting: bool,
    pub session_start_source: Option<String>,
    pub target: ByobuTarget,
    pub received_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodexSession {
    pub session_id: Uuid,
    pub title: Option<String>,
    pub latest_user_message: Option<String>,
    pub latest_user_message_at_unix_ms: Option<i64>,
    pub latest_agent_message: Option<String>,
    pub latest_agent_message_at_unix_ms: Option<i64>,
    pub created_at_unix_ms: Option<i64>,
    pub rollout_updated_at_unix_ms: Option<i64>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub cli_version: Option<String>,
    pub turn_count: u64,
    pub command_count: u64,
    pub file_change_count: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub observations: Vec<CodexSessionObservation>,
}
