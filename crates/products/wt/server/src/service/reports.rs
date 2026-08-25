use super::{map_store_error, Service, WorldWorker};
use crate::service::AgentToolGateway;
use wt_control_protocol::{AgentToolReport, AgentToolReportKind, ApiError, Response, WorldName};

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_agent_tool_reports(&self, owner: &str) -> Result<Response, ApiError> {
        let reports = self
            .store
            .list_agent_tool_reports(owner)
            .map_err(map_store_error)?
            .into_iter()
            .map(|report| {
                Ok(AgentToolReport {
                    world_id: report.world_id,
                    world_name: WorldName::parse(report.world_name).map_err(|error| {
                        ApiError::new(wt_control_protocol::ErrorCode::Internal, error.to_string())
                    })?,
                    kind: match report.kind {
                        wt_workload_registry::AgentToolReportKind::Bug => AgentToolReportKind::Bug,
                        wt_workload_registry::AgentToolReportKind::Issue => {
                            AgentToolReportKind::Issue
                        }
                        wt_workload_registry::AgentToolReportKind::Improvement => {
                            AgentToolReportKind::Improvement
                        }
                        wt_workload_registry::AgentToolReportKind::FeatureRequest => {
                            AgentToolReportKind::FeatureRequest
                        }
                    },
                    description: report.description,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(Response::AgentToolReports { reports })
    }

    pub(super) fn clear_agent_tool_reports(&self, owner: &str) -> Result<Response, ApiError> {
        let count = self
            .store
            .clear_agent_tool_reports(owner)
            .map_err(map_store_error)?;
        Ok(Response::AgentToolReportsCleared { count })
    }
}
