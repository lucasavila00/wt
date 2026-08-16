use super::{map_store_error, Service, WorldWorker};
use crate::service::AgentGitGateway;
use wt_api::{AgentGitReport, AgentGitReportKind, ApiError, InstanceName, Response};

impl<W: WorldWorker, G: AgentGitGateway> Service<W, G> {
    pub(super) fn list_agent_git_reports(&self, owner: &str) -> Result<Response, ApiError> {
        let reports = self
            .store
            .list_agent_git_reports(owner)
            .map_err(map_store_error)?
            .into_iter()
            .map(|report| {
                Ok(AgentGitReport {
                    world_id: report.world_id,
                    world_name: InstanceName::parse(report.world_name).map_err(|error| {
                        ApiError::new(wt_api::ErrorCode::Internal, error.to_string())
                    })?,
                    kind: match report.kind {
                        wt_registry::AgentGitReportKind::Bug => AgentGitReportKind::Bug,
                        wt_registry::AgentGitReportKind::Issue => AgentGitReportKind::Issue,
                        wt_registry::AgentGitReportKind::Improvement => {
                            AgentGitReportKind::Improvement
                        }
                        wt_registry::AgentGitReportKind::FeatureRequest => {
                            AgentGitReportKind::FeatureRequest
                        }
                    },
                    description: report.description,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(Response::AgentGitReports { reports })
    }

    pub(super) fn clear_agent_git_reports(&self, owner: &str) -> Result<Response, ApiError> {
        let count = self
            .store
            .clear_agent_git_reports(owner)
            .map_err(map_store_error)?;
        Ok(Response::AgentGitReportsCleared { count })
    }
}
