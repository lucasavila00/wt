use serde::{Deserialize, Serialize};
use wt_git_smart_protocol::GitService;

pub const PROTOCOL_VERSION: u32 = 11;

pub fn valid_byobu_tmux_session(value: &str) -> bool {
    value == "wt-host"
}

pub fn valid_byobu_pane_id(value: &str) -> bool {
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
    PaneObservations { panes: Vec<PaneObservation> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaneObservation {
    pub tmux_session: String,
    pub pane_id: String,
    pub screen_fingerprint: String,
    pub changed: bool,
}

#[cfg(test)]
mod byobu_target_tests {
    use super::*;

    #[test]
    fn validates_only_wt_byobu_targets() {
        assert!(valid_byobu_tmux_session("wt-host"));
        assert!(!valid_byobu_tmux_session("other"));

        assert!(valid_byobu_pane_id("%0"));
        assert!(valid_byobu_pane_id("%1234567890123456"));
        for invalid in ["", "%", "%a", "1", "%12345678901234567"] {
            assert!(!valid_byobu_pane_id(invalid), "{invalid}");
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
