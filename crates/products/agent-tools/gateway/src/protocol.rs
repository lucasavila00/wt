use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wt_git_smart_protocol::GitService;

pub const PROTOCOL_VERSION: u32 = 9;
pub const CODEX_SESSION_PANE_OPTION: &str = "@wt_codex_session_id";

pub fn valid_codex_tmux_session(value: &str) -> bool {
    value == "wt-host"
}

pub fn valid_codex_pane_id(value: &str) -> bool {
    value.strip_prefix('%').is_some_and(|number| {
        !number.is_empty() && number.len() <= 16 && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ControlRequest {
    Reserve { world_id: String },
    Revoke { grant_id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant: Option<Grant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ControlResponse {
    pub fn ok(grant: Option<Grant>) -> Self {
        Self {
            ok: true,
            grant,
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            grant: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Grant {
    pub id: String,
    pub token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientRequest {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub operation: ClientOperation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ClientOperation {
    Git { service: GitService, source: String },
    Cli { args: Vec<String> },
    CodexSession { event: CodexSessionEvent },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSessionEventKind {
    SessionStart,
    PreCompact,
    PostCompact,
    UserPromptSubmit,
    Stop,
    SessionEnd,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSessionStartSourceKind {
    Startup,
    Resume,
    Clear,
    Compact,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodexSessionStartSource {
    pub kind: CodexSessionStartSourceKind,
    pub raw: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodexSessionEvent {
    pub session_id: Uuid,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub tmux_session: String,
    pub pane_id: String,
    pub kind: CodexSessionEventKind,
    pub pane_generation: u64,
    pub pane_sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_start_source: Option<CodexSessionStartSource>,
}

#[cfg(test)]
mod codex_target_tests {
    use super::*;

    #[test]
    fn validates_only_wt_byobu_targets() {
        assert!(valid_codex_tmux_session("wt-host"));
        assert!(!valid_codex_tmux_session("other"));

        assert!(valid_codex_pane_id("%0"));
        assert!(valid_codex_pane_id("%1234567890123456"));
        for invalid in ["", "%", "%a", "1", "%12345678901234567"] {
            assert!(!valid_codex_pane_id(invalid), "{invalid}");
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportRequest {
    pub protocol_version: u32,
    pub token: String,
    #[serde(flatten)]
    pub operation: ClientOperation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl TransportResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            message: None,
        }
    }

    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            error: None,
            message: Some(message.into()),
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(error.into()),
            message: None,
        }
    }
}
