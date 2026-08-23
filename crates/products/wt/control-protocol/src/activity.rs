use crate::InstanceName;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum GitActivityQuery {
    World {
        world_id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_id: Option<u64>,
    },
    Branch {
        provider_host: String,
        repository: String,
        branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_id: Option<u64>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum WtToolsActivityQuery {
    World {
        world_id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_id: Option<u64>,
    },
    Branch {
        provider_host: String,
        repository: String,
        branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_id: Option<u64>,
    },
    ChangeRequest {
        provider_host: String,
        repository: String,
        change_request: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_id: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitActivityKind {
    Service,
    BranchUpdate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitActivity {
    pub id: u64,
    pub world_id: Uuid,
    pub world_name: InstanceName,
    pub recorded_at_unix_ms: u64,
    pub kind: GitActivityKind,
    pub provider_host: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_oid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_oid: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WtToolsActivity {
    pub id: u64,
    pub world_id: Uuid,
    pub world_name: InstanceName,
    pub recorded_at_unix_ms: u64,
    pub provider_host: String,
    pub repository: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_request: Option<String>,
    pub request_json: String,
    pub response_json: String,
}
