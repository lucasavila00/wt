//! Shared control-plane wire types for `wt` and site helpers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    Create(CreateInstance),
    List,
    Get { name: InstanceName },
    Delete { name: InstanceName },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateInstance {
    pub source: String,
    pub name: InstanceName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
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
            outcome: Outcome::Ok { response },
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
    Ok { response: Response },
    Error { error: ApiError },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
    Instance { instance: Instance },
    Instances { instances: Vec<Instance> },
    Deleted { name: InstanceName },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Instance {
    pub id: Uuid,
    pub name: InstanceName,
    pub owner: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    pub status: InstanceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<SshEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
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
pub struct SshEndpoint {
    pub user: String,
    pub host: String,
    pub port: u16,
}

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
