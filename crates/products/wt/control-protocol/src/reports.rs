use crate::InstanceName;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolReportKind {
    Bug,
    Issue,
    Improvement,
    FeatureRequest,
}

impl fmt::Display for AgentToolReportKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Bug => "bug",
            Self::Issue => "issue",
            Self::Improvement => "improvement",
            Self::FeatureRequest => "feature request",
        };
        formatter.write_str(value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentToolReport {
    pub world_id: Uuid,
    pub world_name: InstanceName,
    pub kind: AgentToolReportKind,
    pub description: String,
}
