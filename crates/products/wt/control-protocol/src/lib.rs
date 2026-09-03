//! Shared control-plane wire types for `wt` and server helpers.

mod activity;
mod capacity;
mod codex;
mod create;
mod mail;
mod pane;
#[cfg(test)]
mod rename_tests;
mod reports;
mod validation;

pub use activity::{
    GitActivity, GitActivityKind, GitActivityQuery, WtToolsActivity, WtToolsActivityQuery,
};
pub use capacity::{Capacity, CapacityResource};
pub use codex::{CodexMessageDelivery, CodexStatus};
pub use create::{validate_create_world_resources, CreateWorld};
pub use mail::{MailKind, WorldMail, MAX_MAIL_TEXT_BYTES, MAX_WORLD_MAIL_PAGE_SIZE};
pub use pane::{
    PaneCell, PaneColor, PaneFrame, PaneObservation, PaneRender, MAX_PANE_CELL_TEXT_BYTES,
    MAX_PANE_FRAME_CELLS, MAX_PANE_FRAME_COLUMNS, MAX_PANE_FRAME_ROWS, MAX_PANE_WINDOW_NAME_BYTES,
};
pub use reports::{AgentToolReport, AgentToolReportKind};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;
pub use validation::{InvalidWorldName, WorldName};
pub use wt_world::WorldId;

pub const PROTOCOL_VERSION: u32 = 20;
pub const BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_COMMIT_SHA: &str = env!("WT_GIT_COMMIT_SHA");
pub const BUILD_DESCRIPTION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("WT_GIT_COMMIT_SHA"),
    ")"
);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildIdentity {
    pub version: String,
    pub commit: String,
}

impl BuildIdentity {
    pub fn current() -> Self {
        Self {
            version: BUILD_VERSION.to_owned(),
            commit: GIT_COMMIT_SHA.to_owned(),
        }
    }
}

impl fmt::Display for BuildIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.version, self.commit)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiProgress {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub event: ProgressEvent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProgressEvent {
    Progress { message: String },
}

impl ApiProgress {
    pub fn new(message: String) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            event: ProgressEvent::Progress { message },
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiRequest {
    pub protocol_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_server_id: Option<Uuid>,
    #[serde(flatten)]
    pub operation: Operation,
}

impl ApiRequest {
    pub fn new(operation: Operation) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            request_hash: None,
            expected_server_id: None,
            operation,
        }
    }

    pub fn with_request_id(
        operation: Operation,
        request_id: Uuid,
        request_hash: String,
        expected_server_id: Option<Uuid>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: Some(request_id),
            request_hash: Some(request_hash),
            expected_server_id,
            operation,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
#[rustfmt::skip]
pub enum Operation {
    ServerInfo,
    CreateWorld(CreateWorld),
    ListWorlds,
    GetWorld { name: WorldName },
    RenameWorld { world_id: WorldId, new_name: WorldName },
    StartWorld { world_id: WorldId },
    StopWorld { world_id: WorldId },
    DeleteWorld { world_id: WorldId },
    StartCodex { world_id: WorldId, message: String },
    InspectCodex { world_id: WorldId, thread_id: String },
    SendCodexMessage { world_id: WorldId, thread_id: String, message: String },
    ListAgentToolReports,
    ClearAgentToolReports,
    ListWorldMail { world_id: WorldId, after_id: u64, limit: u32 },
    ListPaneObservations,
    ListGitActivity { query: GitActivityQuery },
    ListWtToolsActivity { query: WtToolsActivityQuery },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiResponse {
    pub protocol_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<i64>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl ApiResponse {
    pub fn ok(response: Response) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            server_id: None,
            expires_at_unix_ms: None,
            outcome: Outcome::Ok {
                response: Box::new(response),
            },
        }
    }

    pub fn error(error: ApiError) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            server_id: None,
            expires_at_unix_ms: None,
            outcome: Outcome::Error { error },
        }
    }

    pub fn from_outcome(outcome: Outcome) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            server_id: None,
            expires_at_unix_ms: None,
            outcome,
        }
    }

    pub fn with_request_metadata(
        mut self,
        request_id: Uuid,
        server_id: Uuid,
        expires_at_unix_ms: Option<i64>,
    ) -> Self {
        self.request_id = Some(request_id);
        self.server_id = Some(server_id);
        self.expires_at_unix_ms = expires_at_unix_ms;
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Ok { response: Box<Response> },
    Error { error: ApiError },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
    ServerInfo {
        test_server: bool,
        build: BuildIdentity,
    },
    World {
        world: Box<World>,
    },
    Worlds {
        worlds: Vec<World>,
        #[serde(default, skip_serializing_if = "ResourceCapacity::is_empty")]
        capacity: ResourceCapacity,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        disk_usage_bytes: std::collections::BTreeMap<WorldId, u64>,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        agent_tool_report_counts: std::collections::BTreeMap<WorldId, u64>,
    },
    AgentToolReports {
        reports: Vec<AgentToolReport>,
    },
    AgentToolReportsCleared {
        count: u64,
    },
    WorldMail {
        messages: Vec<WorldMail>,
        high_water_id: u64,
    },
    CodexStarted {
        thread_id: String,
        turn_id: String,
        pane_id: String,
        window_name: String,
    },
    CodexInspection {
        thread_id: String,
        status: CodexStatus,
        active_turn_id: Option<String>,
        pane_id: String,
        window_name: String,
        screen: String,
        observed_at_unix_ms: i64,
    },
    CodexMessageSent {
        thread_id: String,
        turn_id: String,
        delivery: CodexMessageDelivery,
    },
    PaneObservations {
        panes: Vec<PaneObservation>,
    },
    GitActivity {
        activity: Vec<GitActivity>,
    },
    WtToolsActivity {
        activity: Vec<WtToolsActivity>,
    },
    WorldDeleted {
        world_id: WorldId,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub vcpus: u64,
    pub memory_mib: u64,
    pub disk_gib: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCapacity {
    pub reserved: Resources,
    pub total: Resources,
}

impl ResourceCapacity {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            reserved: self.reserved.saturating_add(other.reserved),
            total: self.total.saturating_add(other.total),
        }
    }
}

impl Resources {
    fn saturating_add(self, other: Self) -> Self {
        Self {
            vcpus: self.vcpus.saturating_add(other.vcpus),
            memory_mib: self.memory_mib.saturating_add(other.memory_mib),
            disk_gib: self.disk_gib.saturating_add(other.disk_gib),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct World {
    /// Immutable world identity. The same UUID identifies its persistent disk.
    pub world_id: WorldId,
    pub name: WorldName,
    pub owner: String,
    pub status: WorldStatus,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshAccess>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SshAccess {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub host_keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Provisioning,
    Running,
    Stopped,
    Destroying,
    Error,
}

impl fmt::Display for WorldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Provisioning => "provisioning",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Destroying => "destroying",
            Self::Error => "error",
        };
        f.write_str(value)
    }
}

impl FromStr for WorldStatus {
    type Err = ParseWorldStatusError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "provisioning" => Ok(Self::Provisioning),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "destroying" => Ok(Self::Destroying),
            "error" => Ok(Self::Error),
            _ => Err(ParseWorldStatusError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("unknown world status: {0}")]
pub struct ParseWorldStatusError(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<Capacity>,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            capacity: None,
        }
    }

    pub fn capacity(capacity: Capacity) -> Self {
        Self {
            code: ErrorCode::Capacity,
            message: format!("world {} capacity is full", capacity.resource),
            retryable: true,
            capacity: Some(capacity),
        }
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedProtocol,
    ServerMismatch,
    Conflict,
    NotFound,
    Capacity,
    Backend,
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_world_names() {
        for valid in ["repo-feature", "a", "app-123"] {
            assert!(WorldName::parse(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "UPPER",
            "-leading",
            "trailing-",
            "has.dot",
            "has_space",
            "repo-direct",
        ] {
            assert!(WorldName::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn explains_reserved_ssh_alias_suffixes() {
        insta::assert_snapshot!(
            WorldName::parse("repo-direct").unwrap_err().to_string(),
            @"invalid world name: must not end with the reserved SSH alias suffix -direct"
        );
    }

    #[test]
    fn request_has_stable_tagged_shape() {
        let request = ApiRequest::new(Operation::GetWorld {
            name: WorldName::parse("repo-feature").unwrap(),
        });
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "get_world",
                "name": "repo-feature"
            })
        );
    }

    #[test]
    fn start_request_has_stable_shape() {
        let request = ApiRequest::new(Operation::StartWorld {
            world_id: WorldId::from(
                Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            ),
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "start_world",
                "world_id": "123e4567-e89b-12d3-a456-426614174000"
            })
        );
    }

    #[test]
    fn stop_request_has_stable_shape() {
        let request = ApiRequest::new(Operation::StopWorld {
            world_id: WorldId::from(
                Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            ),
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "stop_world",
                "world_id": "123e4567-e89b-12d3-a456-426614174000"
            })
        );
    }

    #[test]
    fn delete_request_targets_the_world_id() {
        let request = ApiRequest::new(Operation::DeleteWorld {
            world_id: WorldId::from(
                Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            ),
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "delete_world",
                "world_id": "123e4567-e89b-12d3-a456-426614174000"
            })
        );
        assert!(serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "operation": "delete_world"
        }))
        .is_err());
    }

    #[test]
    fn pane_observation_list_request_has_a_stable_shape() {
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::ListPaneObservations)).unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "list_pane_observations"
            })
        );
    }

    #[test]
    fn capacity_error_identifies_the_resource() {
        let response = ApiResponse::error(ApiError::capacity(Capacity {
            resource: CapacityResource::Memory,
            total: 32_000,
            reserved: 24_000,
            requested: 8_000,
        }));
        insta::assert_snapshot!(serde_json::to_string_pretty(&response).unwrap(), @r###"
        {
          "protocol_version": 20,
          "outcome": "error",
          "error": {
            "code": "capacity",
            "message": "world memory capacity is full",
            "retryable": true,
            "capacity": {
              "resource": "memory",
              "total": 32000,
              "reserved": 24000,
              "requested": 8000
            }
          }
        }
        "###);
    }

    #[test]
    fn host_create_request_has_tagged_shape() {
        let request = ApiRequest::new(Operation::CreateWorld(CreateWorld {
            name: WorldName::parse("build-world").unwrap(),
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
            git_user_name: "Lucas Ávila".to_owned(),
            git_user_email: "lucaxx@gmail.com".to_owned(),
        }));
        let value = serde_json::to_value(request).unwrap();
        insta::assert_snapshot!(serde_json::to_string_pretty(&value).unwrap(), @r###"
        {
          "disk_gib": 32,
          "git_user_email": "lucaxx@gmail.com",
          "git_user_name": "Lucas Ávila",
          "memory_mib": 4096,
          "name": "build-world",
          "operation": "create_world",
          "protocol_version": 20,
          "vcpus": 2
        }
        "###);
    }

    #[test]
    fn create_request_requires_git_author_identity() {
        let missing = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "operation": "create_world",
            "name": "repo-feature",
        }));
        assert!(missing.is_err());

        let empty = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "operation": "create_world",
            "name": "repo-feature",
            "git_user_name": "",
            "git_user_email": "lucaxx@gmail.com"
        }));
        assert!(empty.is_err());
    }

    #[test]
    fn create_resources_are_strict() {
        let mut request = CreateWorld {
            name: WorldName::parse("sample").unwrap(),
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            git_user_name: "Test User".to_owned(),
            git_user_email: "test@example.invalid".to_owned(),
        };
        assert_eq!(validate_create_world_resources(&request), Ok(()));
        request.vcpus = 0;
        assert!(validate_create_world_resources(&request).is_err());
        request.vcpus = 1;
    }

    #[test]
    fn rejects_invalid_name_from_json() {
        let error = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "operation": "get_world",
            "name": "Not-Valid"
        }))
        .unwrap_err();
        insta::assert_snapshot!(error.to_string(), @"invalid world name: must start with a lowercase letter or digit");
    }

    #[test]
    fn progress_is_a_line_delimited_wire_event() {
        insta::assert_snapshot!(serde_json::to_string_pretty(&ApiProgress::new("Waiting for the guest transport...".into())).unwrap(), @r###"
        {
          "protocol_version": 20,
          "event": "progress",
          "message": "Waiting for the guest transport..."
        }
        "###);
    }

    #[test]
    fn server_info_has_a_stable_shape() {
        let request = ApiRequest::new(Operation::ServerInfo);
        let response = ApiResponse::ok(Response::ServerInfo {
            test_server: true,
            build: BuildIdentity {
                version: "1.2.3".to_owned(),
                commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            },
        });
        insta::assert_snapshot!(serde_json::to_string_pretty(&(request, response)).unwrap(), @r###"
        [
          {
            "protocol_version": 20,
            "operation": "server_info"
          },
          {
            "protocol_version": 20,
            "outcome": "ok",
            "response": {
              "response": "server_info",
              "test_server": true,
              "build": {
                "version": "1.2.3",
                "commit": "0123456789abcdef0123456789abcdef01234567"
              }
            }
          }
        ]
        "###);
    }
}
