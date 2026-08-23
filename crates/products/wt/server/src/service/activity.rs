use super::{map_store_error, Service, WorldWorker};
use crate::service::AgentToolGateway;
use wt_control_protocol::{
    ApiError, GitActivity, GitActivityKind, GitActivityQuery, InstanceName, Response,
    WtToolsActivity, WtToolsActivityQuery,
};

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_git_activity(
        &self,
        owner: &str,
        query: GitActivityQuery,
    ) -> Result<Response, ApiError> {
        let query = match query {
            GitActivityQuery::World {
                world_id,
                before_id,
            } => wt_workload_registry::GitActivityQuery::World {
                world_id,
                before_id,
            },
            GitActivityQuery::Branch {
                provider_host,
                repository,
                branch,
                before_id,
            } => wt_workload_registry::GitActivityQuery::Branch {
                provider_host,
                repository,
                branch,
                before_id,
            },
        };
        let activity = self
            .store
            .list_git_activity(owner, query)
            .map_err(map_store_error)?
            .into_iter()
            .map(|entry| {
                Ok(GitActivity {
                    id: entry.id,
                    world_id: entry.world_id,
                    world_name: parse_world_name(entry.world_name)?,
                    recorded_at_unix_ms: entry.recorded_at_unix_ms,
                    kind: match entry.kind {
                        wt_workload_registry::GitActivityKind::Service => GitActivityKind::Service,
                        wt_workload_registry::GitActivityKind::BranchUpdate => {
                            GitActivityKind::BranchUpdate
                        }
                    },
                    provider_host: entry.provider_host,
                    repository: entry.repository,
                    git_service: entry.git_service,
                    branch: entry.branch,
                    previous_oid: entry.previous_oid,
                    new_oid: entry.new_oid,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(Response::GitActivity { activity })
    }

    pub(super) fn list_wt_tools_activity(
        &self,
        owner: &str,
        query: WtToolsActivityQuery,
    ) -> Result<Response, ApiError> {
        let query = match query {
            WtToolsActivityQuery::World {
                world_id,
                before_id,
            } => wt_workload_registry::WtToolsActivityQuery::World {
                world_id,
                before_id,
            },
            WtToolsActivityQuery::Branch {
                provider_host,
                repository,
                branch,
                before_id,
            } => wt_workload_registry::WtToolsActivityQuery::Branch {
                provider_host,
                repository,
                branch,
                before_id,
            },
            WtToolsActivityQuery::ChangeRequest {
                provider_host,
                repository,
                change_request,
                before_id,
            } => wt_workload_registry::WtToolsActivityQuery::ChangeRequest {
                provider_host,
                repository,
                change_request,
                before_id,
            },
        };
        let activity = self
            .store
            .list_wt_tools_activity(owner, query)
            .map_err(map_store_error)?
            .into_iter()
            .map(|entry| {
                Ok(WtToolsActivity {
                    id: entry.id,
                    world_id: entry.world_id,
                    world_name: parse_world_name(entry.world_name)?,
                    recorded_at_unix_ms: entry.recorded_at_unix_ms,
                    provider_host: entry.provider_host,
                    repository: entry.repository,
                    action: entry.action,
                    branch: entry.branch,
                    change_request: entry.change_request,
                    request_json: entry.request_json,
                    response_json: entry.response_json,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(Response::WtToolsActivity { activity })
    }
}

fn parse_world_name(name: String) -> Result<InstanceName, ApiError> {
    InstanceName::parse(name)
        .map_err(|error| ApiError::new(wt_control_protocol::ErrorCode::Internal, error.to_string()))
}
