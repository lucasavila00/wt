//! Shared control-plane wire types for `wt` and server helpers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

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
    Logs { name: InstanceName, offset: u64 },
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateInstance {
    pub name: InstanceName,
    pub source: String,
    pub git_passphrase: GitPassphrase,
}

#[derive(Deserialize, Eq, PartialEq, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct GitPassphrase(String);

impl GitPassphrase {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GitPassphrase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GitPassphrase([REDACTED])")
    }
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
    Logs {
        chunk: String,
        next_offset: u64,
        status: InstanceStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Instance {
    pub id: Uuid,
    pub name: InstanceName,
    pub owner: String,
    pub status: InstanceStatus,
    pub source: String,
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
    Destroying,
    Error,
}

impl fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Provisioning => "provisioning",
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
    InvalidGitPassphrase,
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
    fn create_request_has_server_credentials_shape() {
        let request = ApiRequest::new(Operation::Create(CreateInstance {
            name: InstanceName::parse("repo-feature").unwrap(),
            source: "git@github.com:example/repo.git".to_owned(),
            git_passphrase: GitPassphrase::new("secret".to_owned()),
        }));
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": 1,
                "operation": "create",
                "name": "repo-feature",
                "source": "git@github.com:example/repo.git",
                "git_passphrase": "secret"
            })
        );
    }

    #[test]
    fn git_passphrase_debug_is_redacted() {
        let passphrase = GitPassphrase::new("do-not-print".to_owned());
        let debug = format!("{passphrase:?}");
        assert!(!debug.contains(passphrase.expose_secret()));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn rejects_invalid_name_from_json() {
        let error = serde_json::from_value::<ApiRequest>(serde_json::json!({
            "protocol_version": 1,
            "operation": "get",
            "name": "Not-Valid"
        }))
        .unwrap_err();
        assert!(error.to_string().contains("invalid instance name"));
    }
}
