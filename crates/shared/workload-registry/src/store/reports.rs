use super::{map_registry_error, Store, StoreError};
use crate::{AgentToolReport, CodexSessionReport};
use std::collections::BTreeMap;
use uuid::Uuid;

impl Store {
    pub fn list_codex_session_reports(
        &self,
        owner: &str,
    ) -> Result<Vec<CodexSessionReport>, StoreError> {
        self.registry
            .list_codex_session_reports(owner)
            .map_err(map_registry_error)
    }

    pub fn list_agent_tool_reports(&self, owner: &str) -> Result<Vec<AgentToolReport>, StoreError> {
        self.registry
            .list_agent_tool_reports(owner)
            .map_err(map_registry_error)
    }

    pub fn agent_tool_report_counts(&self, owner: &str) -> Result<BTreeMap<Uuid, u64>, StoreError> {
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
