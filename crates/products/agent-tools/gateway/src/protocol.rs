use serde::{Deserialize, Serialize};
use wt_control_protocol::PaneFrame;
use wt_git_smart_protocol::GitService;

pub const PROTOCOL_VERSION: u32 = 13;
pub const MAX_PANE_OBSERVATIONS: usize = 32;
pub const MAX_PANE_OBSERVATION_REPORT_BYTES: usize = 2_000_000;

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
    pub window_index: i64,
    pub window_name: String,
    pub screen_fingerprint: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub frame: PaneFrame,
}

pub fn validate_pane_observations(panes: &[PaneObservation]) -> Result<(), String> {
    if panes.len() > MAX_PANE_OBSERVATIONS {
        return Err("too many pane observations".into());
    }
    if serde_json::to_vec(panes)
        .map_err(|error| format!("encode pane observations: {error}"))?
        .len()
        > MAX_PANE_OBSERVATION_REPORT_BYTES
    {
        return Err("pane observation report is too large".into());
    }
    let mut targets = std::collections::BTreeSet::new();
    for pane in panes {
        if !valid_byobu_tmux_session(&pane.tmux_session)
            || !valid_byobu_pane_id(&pane.pane_id)
            || pane.window_index < 0
            || !valid_display_text(&pane.window_name, 255)
            || pane.screen_fingerprint.len() != 64
            || !pane
                .screen_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !valid_display_text(&pane.cwd, 4096)
            || pane
                .git_branch
                .as_deref()
                .is_some_and(|branch| !valid_display_text(branch, 255))
            || !targets.insert((&pane.tmux_session, &pane.pane_id))
        {
            return Err("invalid pane observation".into());
        }
        pane.frame
            .validate()
            .map_err(|error| format!("invalid pane frame: {error}"))?;
    }
    Ok(())
}

fn valid_display_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
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
