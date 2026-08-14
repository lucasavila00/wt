//! Shared control-plane wire types for `wt` and server helpers.

mod validation;

pub use validation::{
    validate_git_branch, validate_ssh_git_source, InstanceName, InvalidGitBranch, InvalidGitSource,
    InvalidInstanceName,
};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const WT_GIT_COMMIT: &str = env!("WT_GIT_COMMIT");

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiRequest {
    pub protocol_version: u32,
    #[serde(deserialize_with = "deserialize_git_commit")]
    pub client_commit: String,
    #[serde(flatten)]
    pub operation: Operation,
}

impl ApiRequest {
    pub fn new(operation: Operation) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            client_commit: WT_GIT_COMMIT.to_owned(),
            operation,
        }
    }
}

fn deserialize_git_commit<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(serde::de::Error::custom(
            "client_commit must be a full lowercase Git commit hash",
        ));
    }
    Ok(value)
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    Create(CreateInstance),
    Fork(ForkInstance),
    List,
    Get { name: InstanceName },
    Start { name: InstanceName },
    Delete { name: InstanceName },
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ForkInstance {
    pub source: InstanceName,
    pub name: InstanceName,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateInstance {
    pub name: InstanceName,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    pub ssh_authorized_keys: Vec<String>,
    #[serde(flatten)]
    pub application: CreateApplication,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CreateApplication {
    Devcontainer {
        source: String,
        #[serde(deserialize_with = "deserialize_git_branch")]
        git_base: String,
        #[serde(deserialize_with = "deserialize_nonempty_string")]
        git_user_name: String,
        #[serde(deserialize_with = "deserialize_nonempty_string")]
        git_user_email: String,
    },
    Host {
        #[serde(deserialize_with = "deserialize_nonempty_string")]
        user_data: String,
    },
}

impl CreateInstance {
    pub fn kind(&self) -> WorldKind {
        self.application.kind()
    }
}

impl CreateApplication {
    pub fn kind(&self) -> WorldKind {
        match self {
            Self::Devcontainer { .. } => WorldKind::Devcontainer,
            Self::Host { .. } => WorldKind::Host,
        }
    }
}

pub fn validate_create_resources(request: &CreateInstance) -> Result<(), &'static str> {
    if request.vcpus == 0 || request.memory_mib == 0 || request.disk_gib == 0 {
        return Err("CPU, memory, and disk values must be greater than zero");
    }
    if request.ssh_authorized_keys.is_empty() {
        return Err("at least one SSH authorized key is required");
    }
    let mut unique = std::collections::BTreeSet::new();
    for key in &request.ssh_authorized_keys {
        let mut parsed = ssh_key::PublicKey::from_openssh(key)
            .map_err(|_| "SSH authorized keys must be valid OpenSSH public keys")?;
        parsed.set_comment("");
        let normalized = parsed
            .to_openssh()
            .map_err(|_| "SSH authorized keys must be valid OpenSSH public keys")?;
        if !unique.insert(normalized) {
            return Err("SSH authorized keys must not contain duplicates");
        }
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

fn deserialize_git_branch<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_git_branch(&value).map_err(serde::de::Error::custom)?;
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
    Instance { instance: Box<Instance> },
    Instances { instances: Vec<Instance> },
    Deleted { name: InstanceName },
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
    #[serde(flatten)]
    pub application: InstanceApplication,
}

impl Instance {
    pub fn kind(&self) -> WorldKind {
        self.application.kind()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InstanceApplication {
    Devcontainer {
        source: String,
        git_base: String,
        git_prefix: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_ssh: Option<AppSshAccess>,
    },
    Host,
}

impl InstanceApplication {
    pub fn kind(&self) -> WorldKind {
        match self {
            Self::Devcontainer { .. } => WorldKind::Devcontainer,
            Self::Host => WorldKind::Host,
        }
    }

    pub fn app_ssh(&self) -> Option<&AppSshAccess> {
        match self {
            Self::Devcontainer { app_ssh, .. } => app_ssh.as_ref(),
            Self::Host => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorldKind {
    Devcontainer,
    Host,
    GithubCi,
}

impl fmt::Display for WorldKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Devcontainer => "devcontainer",
            Self::Host => "host",
            Self::GithubCi => "github-ci",
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SshAccess {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub host_keys: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppSshAccess {
    pub user: String,
    pub port: u16,
    pub host_keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Provisioning,
    Setup,
    Running,
    Stopped,
    Destroying,
    Error,
}

impl fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Provisioning => "provisioning",
            Self::Setup => "setup",
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
            "setup" => Ok(Self::Setup),
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
            "repo-host",
            "repo-vs",
        ] {
            assert!(InstanceName::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn explains_reserved_ssh_alias_suffixes() {
        insta::assert_snapshot!(
            InstanceName::parse("repo-vs").unwrap_err().to_string(),
            @"invalid instance name: must not end with the reserved SSH alias suffix -host or -vs"
        );
    }

    #[test]
    fn validates_only_ssh_git_sources() {
        for valid in [
            "git@github.com:example/repo.git",
            "ssh://git@example.test/repo.git",
            "ssh://git@example.test:2222/repo.git",
        ] {
            assert!(validate_ssh_git_source(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "https://example.test/repo.git",
            "git://example.test/repo.git",
            "/tmp/repo.git",
            "ssh://example.test",
            "git@:repo.git",
            "git@example.test:",
        ] {
            assert!(validate_ssh_git_source(invalid).is_err(), "{invalid}");
        }
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
                "protocol_version": 1,
                "client_commit": WT_GIT_COMMIT,
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
                "protocol_version": 1,
                "client_commit": WT_GIT_COMMIT,
                "operation": "start",
                "name": "repo-feature"
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
          "protocol_version": 1,
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
    fn create_request_has_setup_shape() {
        let request = ApiRequest::new(Operation::Create(CreateInstance {
            name: InstanceName::parse("repo-feature").unwrap(),
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
            ssh_authorized_keys: vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example".to_owned()],
            application: CreateApplication::Devcontainer {
                source: "git@github.com:example/repo.git".to_owned(),
                git_base: "devcontainer".to_owned(),
                git_user_name: "Lucas Ávila".to_owned(),
                git_user_email: "lucaxx@gmail.com".to_owned(),
            },
        }));
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": 1,
                "client_commit": WT_GIT_COMMIT,
                "operation": "create",
                "kind": "devcontainer",
                "name": "repo-feature",
                "source": "git@github.com:example/repo.git",
                "git_base": "devcontainer",
                "git_user_name": "Lucas Ávila",
                "git_user_email": "lucaxx@gmail.com",
                "vcpus": 2,
                "memory_mib": 4096,
                "disk_gib": 32,
                "ssh_authorized_keys": ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example"]
            })
        );
    }

    #[test]
    fn host_create_request_has_tagged_shape() {
        let request = ApiRequest::new(Operation::Create(CreateInstance {
            name: InstanceName::parse("build-world").unwrap(),
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
            ssh_authorized_keys: vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example".to_owned()],
            application: CreateApplication::Host {
                user_data: "#cloud-config\nruncmd:\n  - touch /ready\n".to_owned(),
            },
        }));
        let mut value = serde_json::to_value(request).unwrap();
        value["client_commit"] = "<commit>".into();
        insta::assert_snapshot!(serde_json::to_string_pretty(&value).unwrap(), @r###"
        {
          "client_commit": "<commit>",
          "disk_gib": 32,
          "kind": "host",
          "memory_mib": 4096,
          "name": "build-world",
          "operation": "create",
          "protocol_version": 1,
          "ssh_authorized_keys": [
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example"
          ],
          "user_data": "#cloud-config\nruncmd:\n  - touch /ready\n",
          "vcpus": 2
        }
        "###);
    }

    #[test]
    fn create_request_requires_git_author_identity() {
        let missing = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 1,
            "client_commit": WT_GIT_COMMIT,
            "operation": "create",
            "kind": "devcontainer",
            "name": "repo-feature",
            "source": "git@github.com:example/repo.git",
            "git_base": "main",
        }));
        assert!(missing.is_err());

        let empty = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 1,
            "client_commit": WT_GIT_COMMIT,
            "operation": "create",
            "kind": "devcontainer",
            "name": "repo-feature",
            "source": "git@github.com:example/repo.git",
            "git_base": "main",
            "git_user_name": "",
            "git_user_email": "lucaxx@gmail.com"
        }));
        assert!(empty.is_err());
    }

    #[test]
    fn create_resources_and_authorized_keys_are_strict() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example";
        let mut request = CreateInstance {
            name: InstanceName::parse("sample").unwrap(),
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            ssh_authorized_keys: vec![key.to_owned()],
            application: CreateApplication::Devcontainer {
                source: "git@example.test:repo.git".to_owned(),
                git_base: "main".to_owned(),
                git_user_name: "Test User".to_owned(),
                git_user_email: "test@example.invalid".to_owned(),
            },
        };
        assert_eq!(validate_create_resources(&request), Ok(()));
        request.vcpus = 0;
        assert!(validate_create_resources(&request).is_err());
        request.vcpus = 1;
        request.ssh_authorized_keys.push(key.to_owned());
        assert!(validate_create_resources(&request).is_err());
    }

    #[test]
    fn rejects_invalid_name_from_json() {
        let error = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 1,
            "client_commit": WT_GIT_COMMIT,
            "operation": "get",
            "name": "Not-Valid"
        }))
        .unwrap_err();
        insta::assert_snapshot!(error.to_string(), @"invalid instance name: must start with a lowercase letter or digit");
    }

    #[test]
    fn request_requires_a_full_lowercase_client_commit() {
        for client_commit in [
            None,
            Some("abc"),
            Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        ] {
            let mut value = serde_json::json!({
                "protocol_version": 1,
                "operation": "list"
            });
            if let Some(client_commit) = client_commit {
                value["client_commit"] = client_commit.into();
            }
            assert!(serde_json::from_value::<ApiRequest>(value).is_err());
        }
    }
}
