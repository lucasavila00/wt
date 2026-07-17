//! Shared control-plane wire types for `wt` and server helpers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;

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
    Create(CreateInstance),
    List,
    Get { name: InstanceName },
    Delete { name: InstanceName },
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateInstance {
    pub name: InstanceName,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub git_user_name: String,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub git_user_email: String,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    pub ssh_authorized_keys: Vec<String>,
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
    pub source: String,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshAccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_ssh: Option<AppSshAccess>,
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
    Destroying,
    Error,
}

impl fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Provisioning => "provisioning",
            Self::Setup => "setup",
            Self::Running => "running",
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
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedProtocol,
    Conflict,
    NotFound,
    Backend,
    Internal,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct InstanceName(String);

impl InstanceName {
    pub fn parse(value: impl Into<String>) -> Result<Self, InvalidInstanceName> {
        let value = value.into();
        validate_instance_name(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for InstanceName {
    type Err = InvalidInstanceName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for InstanceName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invalid instance name: {reason}")]
pub struct InvalidInstanceName {
    reason: &'static str,
}

pub fn validate_ssh_git_source(value: &str) -> Result<(), InvalidGitSource> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == 0)
    {
        return Err(InvalidGitSource);
    }
    if let Some(rest) = value.strip_prefix("ssh://") {
        let Some((authority, path)) = rest.split_once('/') else {
            return Err(InvalidGitSource);
        };
        if !authority.is_empty() && !path.is_empty() {
            return Ok(());
        }
        return Err(InvalidGitSource);
    }
    let Some((authority, path)) = value.split_once(':') else {
        return Err(InvalidGitSource);
    };
    let Some((user, host)) = authority.split_once('@') else {
        return Err(InvalidGitSource);
    };
    if user.is_empty() || host.is_empty() || host.contains('@') || path.is_empty() {
        return Err(InvalidGitSource);
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("source must be an ssh:// or user@host:path Git URL")]
pub struct InvalidGitSource;

fn validate_instance_name(value: &str) -> Result<(), InvalidInstanceName> {
    if value.is_empty() || value.len() > 63 {
        return Err(InvalidInstanceName {
            reason: "must contain 1 to 63 characters",
        });
    }
    if !value.as_bytes()[0].is_ascii_lowercase() && !value.as_bytes()[0].is_ascii_digit() {
        return Err(InvalidInstanceName {
            reason: "must start with a lowercase letter or digit",
        });
    }
    if !value.as_bytes()[value.len() - 1].is_ascii_lowercase()
        && !value.as_bytes()[value.len() - 1].is_ascii_digit()
    {
        return Err(InvalidInstanceName {
            reason: "must end with a lowercase letter or digit",
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(InvalidInstanceName {
            reason: "only lowercase letters, digits, and hyphens are allowed",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_instance_names() {
        for valid in ["repo-feature", "a", "app-123"] {
            assert!(InstanceName::parse(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "UPPER", "-leading", "trailing-", "has.dot", "has_space"] {
            assert!(InstanceName::parse(invalid).is_err(), "{invalid}");
        }
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
                "operation": "get",
                "name": "repo-feature"
            })
        );
    }

    #[test]
    fn create_request_has_setup_shape() {
        let request = ApiRequest::new(Operation::Create(CreateInstance {
            name: InstanceName::parse("repo-feature").unwrap(),
            source: "git@github.com:example/repo.git".to_owned(),
            git_branch: None,
            git_ref: Some("devcontainer".to_owned()),
            git_user_name: "Lucas Ávila".to_owned(),
            git_user_email: "lucaxx@gmail.com".to_owned(),
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
            ssh_authorized_keys: vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example".to_owned()],
        }));
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": 1,
                "operation": "create",
                "name": "repo-feature",
                "source": "git@github.com:example/repo.git",
                "git_ref": "devcontainer",
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
    fn create_request_requires_git_author_identity() {
        let missing = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 1,
            "operation": "create",
            "name": "repo-feature",
            "source": "git@github.com:example/repo.git",
        }));
        assert!(missing.is_err());

        let empty = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 1,
            "operation": "create",
            "name": "repo-feature",
            "source": "git@github.com:example/repo.git",
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
            source: "git@example.test:repo.git".to_owned(),
            git_branch: None,
            git_ref: None,
            git_user_name: "Test User".to_owned(),
            git_user_email: "test@example.invalid".to_owned(),
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            ssh_authorized_keys: vec![key.to_owned()],
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
            "operation": "get",
            "name": "Not-Valid"
        }))
        .unwrap_err();
        insta::assert_snapshot!(error.to_string(), @"invalid instance name: must start with a lowercase letter or digit");
    }
}
