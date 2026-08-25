use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorldName(String);

impl WorldName {
    pub fn parse(value: impl Into<String>) -> Result<Self, InvalidWorldName> {
        let value = value.into();
        validate_world_name(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for WorldName {
    type Err = InvalidWorldName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for WorldName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invalid world name: {reason}")]
pub struct InvalidWorldName {
    reason: &'static str,
}

fn validate_world_name(value: &str) -> Result<(), InvalidWorldName> {
    if value.is_empty() || value.len() > 63 {
        return Err(InvalidWorldName {
            reason: "must contain 1 to 63 characters",
        });
    }
    if !value.as_bytes()[0].is_ascii_lowercase() && !value.as_bytes()[0].is_ascii_digit() {
        return Err(InvalidWorldName {
            reason: "must start with a lowercase letter or digit",
        });
    }
    if !value.as_bytes()[value.len() - 1].is_ascii_lowercase()
        && !value.as_bytes()[value.len() - 1].is_ascii_digit()
    {
        return Err(InvalidWorldName {
            reason: "must end with a lowercase letter or digit",
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(InvalidWorldName {
            reason: "only lowercase letters, digits, and hyphens are allowed",
        });
    }
    if value.ends_with("-direct") {
        return Err(InvalidWorldName {
            reason: "must not end with the reserved SSH alias suffix -direct",
        });
    }
    Ok(())
}
