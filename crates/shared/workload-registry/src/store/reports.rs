use super::{map_registry_error, Store, StoreError};
use crate::{AgentToolReport, PaneObservation};
use std::collections::BTreeMap;
use wt_world::WorldId;

impl Store {
    pub fn list_pane_observations(&self, owner: &str) -> Result<Vec<PaneObservation>, StoreError> {
        self.registry
            .list_pane_observations(owner)
            .map_err(map_registry_error)
    }

    pub fn list_agent_tool_reports(&self, owner: &str) -> Result<Vec<AgentToolReport>, StoreError> {
        self.registry
            .list_agent_tool_reports(owner)
            .map_err(map_registry_error)
    }

    pub fn agent_tool_report_counts(
        &self,
        owner: &str,
    ) -> Result<BTreeMap<WorldId, u64>, StoreError> {
        self.registry
            .agent_tool_report_counts(owner)
            .map_err(map_registry_error)
    }

    pub fn clear_agent_tool_reports(&self, owner: &str) -> Result<u64, StoreError> {
        self.registry
            .clear_agent_tool_reports(owner)
            .map_err(map_registry_error)
    }
}
