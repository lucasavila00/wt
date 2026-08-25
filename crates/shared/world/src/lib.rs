use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// The immutable identity of one WT world and its durable disk.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorldId(Uuid);

impl WorldId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for WorldId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for WorldId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<WorldId> for Uuid {
    fn from(value: WorldId) -> Self {
        value.0
    }
}

impl fmt::Display for WorldId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for WorldId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}
