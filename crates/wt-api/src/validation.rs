use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

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

pub fn validate_git_branch(value: &str) -> Result<(), InvalidGitBranch> {
    if value.is_empty()
        || value == "@"
        || value.starts_with('.')
        || value.starts_with('/')
        || value.ends_with('.')
        || value.ends_with('/')
        || value.contains("..")
        || value.contains("@{")
        || value.contains("//")
        || value.split('/').any(|part| part.ends_with(".lock"))
        || value.bytes().any(|byte| {
            byte.is_ascii_control()
                || matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
    {
        return Err(InvalidGitBranch);
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invalid Git branch name")]
pub struct InvalidGitBranch;

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
    if value.ends_with("-host") || value.ends_with("-vs") {
        return Err(InvalidInstanceName {
            reason: "must not end with the reserved SSH alias suffix -host or -vs",
        });
    }
    Ok(())
}
