use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodexStart {
    pub thread_id: String,
    pub turn_id: String,
    pub pane_id: String,
    pub window_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodexInspection {
    pub status: CodexRuntimeStatus,
    pub active_turn_id: Option<String>,
    pub pane_id: String,
    pub window_name: String,
    pub screen: String,
    pub observed_at_unix_ms: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexRuntimeStatus {
    Active,
    Idle,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodexSend {
    pub turn_id: String,
    pub delivery: CodexMessageDelivery,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexMessageDelivery {
    Steered,
    Started,
}
