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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryGitStateQuery {
    pub provider_host: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_before_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wt_tools_before_id: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryCheckoutState {
    pub world_id: Uuid,
    pub world_name: InstanceName,
    pub session_id: Uuid,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub checked_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryGitState {
    pub repository_id: u64,
    pub provider_host: String,
    pub repository: String,
    pub checkouts: Vec<RepositoryCheckoutState>,
    pub git_activity: Vec<GitActivity>,
    pub wt_tools_activity: Vec<WtToolsActivity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApiRequest, Operation, PROTOCOL_VERSION};

    #[test]
    fn repository_git_state_request_has_independent_activity_cursors() {
        let request = ApiRequest::new(Operation::RepositoryGitState {
            query: RepositoryGitStateQuery {
                provider_host: "github.com".into(),
                repository: "acme/project".into(),
                git_before_id: Some(42),
                wt_tools_before_id: Some(17),
            },
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "repository_git_state",
                "query": {
                    "provider_host": "github.com",
                    "repository": "acme/project",
                    "git_before_id": 42,
                    "wt_tools_before_id": 17
                }
            })
        );
    }
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
