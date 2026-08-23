//! Shared control-plane wire types for `wt` and server helpers.

mod codex;
mod reports;
mod validation;

pub use codex::{ByobuTarget, CodexSession, CodexSessionObservation, CodexSessionState};
pub use reports::{AgentToolReport, AgentToolReportKind};

pub use validation::{InstanceName, InvalidInstanceName};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 9;
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
    #[serde(flatten)]
    pub operation: Operation,
}

impl ApiRequest {
    pub fn new(operation: Operation) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            operation,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    ServerInfo,
    Create(CreateInstance),
    List,
    Get { name: InstanceName },
    Start { name: InstanceName },
    Stop { name: InstanceName },
    Delete { name: InstanceName },
    ListAgentToolReports,
    ClearAgentToolReports,
    ListCodexSessions,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateInstance {
    pub name: InstanceName,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub git_user_name: String,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub git_user_email: String,
}

pub fn validate_create_resources(request: &CreateInstance) -> Result<(), &'static str> {
    if request.vcpus == 0 || request.memory_mib == 0 || request.disk_gib == 0 {
        return Err("CPU, memory, and disk values must be greater than zero");
    }
    Ok(())
}

fn deserialize_nonempty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("value must not be empty"));
    }
    Ok(value)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiResponse {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl ApiResponse {
    pub fn ok(response: Response) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            outcome: Outcome::Ok {
                response: Box::new(response),
            },
        }
    }

    pub fn error(error: ApiError) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            outcome: Outcome::Error { error },
        }
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
    Instance {
        instance: Box<Instance>,
    },
    Instances {
        instances: Vec<Instance>,
        #[serde(default, skip_serializing_if = "ResourceCapacity::is_empty")]
        capacity: ResourceCapacity,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        disk_usage_bytes: std::collections::BTreeMap<Uuid, u64>,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        agent_tool_report_counts: std::collections::BTreeMap<Uuid, u64>,
    },
    AgentToolReports {
        reports: Vec<AgentToolReport>,
    },
    AgentToolReportsCleared {
        count: u64,
    },
    CodexSessions {
        sessions: Vec<CodexSession>,
    },
    Deleted {
        name: InstanceName,
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
pub struct Instance {
    pub id: Uuid,
    pub name: InstanceName,
    pub owner: String,
    pub status: InstanceStatus,
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
pub enum InstanceStatus {
    Provisioning,
    Running,
    Stopped,
    Destroying,
    Error,
}

impl fmt::Display for InstanceStatus {
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

impl FromStr for InstanceStatus {
    type Err = ParseStatusError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "provisioning" => Ok(Self::Provisioning),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "destroying" => Ok(Self::Destroying),
            "error" => Ok(Self::Error),
            _ => Err(ParseStatusError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("unknown instance status: {0}")]
pub struct ParseStatusError(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<Capacity>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            capacity: None,
        }
    }

    pub fn capacity(capacity: Capacity) -> Self {
        Self {
            code: ErrorCode::Capacity,
            message: format!("world {} capacity is full", capacity.resource),
            capacity: Some(capacity),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    pub resource: CapacityResource,
    pub total: u64,
    pub reserved: u64,
    pub requested: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityResource {
    Cpu,
    Memory,
    Disk,
}

impl fmt::Display for CapacityResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cpu => "CPU",
            Self::Memory => "memory",
            Self::Disk => "disk",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedProtocol,
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
    fn validates_instance_names() {
        for valid in ["repo-feature", "a", "app-123"] {
            assert!(InstanceName::parse(valid).is_ok(), "{valid}");
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
            assert!(InstanceName::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn explains_reserved_ssh_alias_suffixes() {
        insta::assert_snapshot!(
            InstanceName::parse("repo-direct").unwrap_err().to_string(),
            @"invalid instance name: must not end with the reserved SSH alias suffix -direct"
        );
    }

    #[test]
    fn request_has_stable_tagged_shape() {
        let request = ApiRequest::new(Operation::Get {
            name: InstanceName::parse("repo-feature").unwrap(),
        });
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "protocol_version": 9,
                "operation": "get",
                "name": "repo-feature"
            })
        );
    }

    #[test]
    fn start_request_has_stable_shape() {
        let request = ApiRequest::new(Operation::Start {
            name: InstanceName::parse("repo-feature").unwrap(),
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": 9,
                "operation": "start",
                "name": "repo-feature"
            })
        );
    }

    #[test]
    fn stop_request_has_stable_shape() {
        let request = ApiRequest::new(Operation::Stop {
            name: InstanceName::parse("repo-feature").unwrap(),
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": 9,
                "operation": "stop",
                "name": "repo-feature"
            })
        );
    }

    #[test]
    fn live_codex_session_has_a_complete_pane_target() {
        let session = CodexSession {
            session_id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            title: Some("Improve session cards".into()),
            latest_user_message: Some("Show the latest user request on the card".into()),
            latest_user_message_at_unix_ms: Some(39),
            latest_agent_message: Some("The session card is ready".into()),
            latest_agent_message_at_unix_ms: Some(40),
            created_at_unix_ms: Some(10),
            rollout_updated_at_unix_ms: Some(40),
            cwd: Some("/home/wt/project".into()),
            model: Some("gpt-5.6-sol".into()),
            cli_version: Some("0.149.0".into()),
            turn_count: 3,
            command_count: 4,
            file_change_count: 2,
            input_tokens: 1_000,
            cached_input_tokens: 800,
            output_tokens: 200,
            reasoning_output_tokens: 50,
            observations: vec![CodexSessionObservation {
                world_id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174001").unwrap(),
                world_name: InstanceName::parse("checkout").unwrap(),
                cwd: "/home/wt/project".into(),
                repository_root: Some("/home/wt/project".into()),
                repository_url: Some("git@github.com:acme/project.git".into()),
                git_branch: Some("wt/session-cards".into()),
                state: CodexSessionState::Unknown,
                session_start_source: Some("compact".into()),
                target: ByobuTarget {
                    tmux_session: "wt-host".into(),
                    pane_id: "%3".into(),
                },
                received_at_unix_ms: 42,
            }],
        };

        insta::assert_snapshot!(serde_json::to_string_pretty(&session).unwrap(), @r###"
        {
          "session_id": "123e4567-e89b-12d3-a456-426614174000",
          "title": "Improve session cards",
          "latest_user_message": "Show the latest user request on the card",
          "latest_user_message_at_unix_ms": 39,
          "latest_agent_message": "The session card is ready",
          "latest_agent_message_at_unix_ms": 40,
          "created_at_unix_ms": 10,
          "rollout_updated_at_unix_ms": 40,
          "cwd": "/home/wt/project",
          "model": "gpt-5.6-sol",
          "cli_version": "0.149.0",
          "turn_count": 3,
          "command_count": 4,
          "file_change_count": 2,
          "input_tokens": 1000,
          "cached_input_tokens": 800,
          "output_tokens": 200,
          "reasoning_output_tokens": 50,
          "observations": [
            {
              "world_id": "123e4567-e89b-12d3-a456-426614174001",
              "world_name": "checkout",
              "cwd": "/home/wt/project",
              "repository_root": "/home/wt/project",
              "repository_url": "git@github.com:acme/project.git",
              "git_branch": "wt/session-cards",
              "state": "unknown",
              "session_start_source": "compact",
              "target": {
                "tmux_session": "wt-host",
                "pane_id": "%3"
              },
              "received_at_unix_ms": 42
            }
          ]
        }
        "###);
    }

    #[test]
    fn codex_session_list_request_has_a_stable_shape() {
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::ListCodexSessions)).unwrap(),
            serde_json::json!({
                "protocol_version": 9,
                "operation": "list_codex_sessions"
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
          "protocol_version": 9,
          "outcome": "error",
          "error": {
            "code": "capacity",
            "message": "world memory capacity is full",
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
        let request = ApiRequest::new(Operation::Create(CreateInstance {
            name: InstanceName::parse("build-world").unwrap(),
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
          "operation": "create",
          "protocol_version": 9,
          "vcpus": 2
        }
        "###);
    }

    #[test]
    fn create_request_requires_git_author_identity() {
        let missing = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 9,
            "operation": "create",
            "name": "repo-feature",
        }));
        assert!(missing.is_err());

        let empty = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 9,
            "operation": "create",
            "name": "repo-feature",
            "git_user_name": "",
            "git_user_email": "lucaxx@gmail.com"
        }));
        assert!(empty.is_err());
    }

    #[test]
    fn create_resources_are_strict() {
        let mut request = CreateInstance {
            name: InstanceName::parse("sample").unwrap(),
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            git_user_name: "Test User".to_owned(),
            git_user_email: "test@example.invalid".to_owned(),
        };
        assert_eq!(validate_create_resources(&request), Ok(()));
        request.vcpus = 0;
        assert!(validate_create_resources(&request).is_err());
        request.vcpus = 1;
    }

    #[test]
    fn rejects_invalid_name_from_json() {
        let error = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 9,
            "operation": "get",
            "name": "Not-Valid"
        }))
        .unwrap_err();
        insta::assert_snapshot!(error.to_string(), @"invalid instance name: must start with a lowercase letter or digit");
    }

    #[test]
    fn progress_is_a_line_delimited_wire_event() {
        insta::assert_snapshot!(serde_json::to_string_pretty(&ApiProgress::new("Waiting for the guest transport...".into())).unwrap(), @r###"
        {
          "protocol_version": 9,
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
            "protocol_version": 9,
            "operation": "server_info"
          },
          {
            "protocol_version": 9,
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
