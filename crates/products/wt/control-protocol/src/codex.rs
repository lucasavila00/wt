use crate::InstanceName;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub world_id: Uuid,
    pub world_name: InstanceName,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_context_checked_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_context_error: Option<String>,
    pub state: CodexSessionState,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_compacting: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_start_source: Option<String>,
    pub target: ByobuTarget,
    pub received_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodexSession {
    pub session_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_user_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_user_message_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_agent_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_agent_message_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_updated_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub turn_count: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub command_count: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub file_change_count: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub input_tokens: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cached_input_tokens: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reasoning_output_tokens: u64,
    pub observations: Vec<CodexSessionObservation>,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}
