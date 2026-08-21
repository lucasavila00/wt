use super::{map_registry_error, Store, StoreError};
use crate::AgentGitReport;
use std::collections::BTreeMap;
use uuid::Uuid;

impl Store {
    pub fn list_agent_git_reports(&self, owner: &str) -> Result<Vec<AgentGitReport>, StoreError> {
        self.registry
            .list_agent_git_reports(owner)
            .map_err(map_registry_error)
    }

    pub fn agent_git_report_counts(&self, owner: &str) -> Result<BTreeMap<Uuid, u64>, StoreError> {
        self.registry
            .agent_git_report_counts(owner)
            .map_err(map_registry_error)
    }

    pub fn clear_agent_git_reports(&self, owner: &str) -> Result<u64, StoreError> {
        self.registry
            .clear_agent_git_reports(owner)
            .map_err(map_registry_error)
    }
}
