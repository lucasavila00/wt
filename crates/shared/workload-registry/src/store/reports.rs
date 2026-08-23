use super::{map_registry_error, Store, StoreError};
use crate::{
    AgentToolReport, CodexSessionCatalogEntry, CodexSessionCatalogInput, CodexSessionReport,
};
use std::collections::{BTreeMap, BTreeSet};
use wt_world::WorldId;

impl Store {
    pub fn upsert_codex_session_catalog(
        &self,
        entry: &CodexSessionCatalogInput,
    ) -> Result<(), StoreError> {
        self.registry
            .upsert_codex_session_catalog(entry)
            .map_err(map_registry_error)
    }

    pub fn list_codex_session_catalog(&self) -> Result<Vec<CodexSessionCatalogEntry>, StoreError> {
        self.registry
            .list_codex_session_catalog()
            .map_err(map_registry_error)
    }

    pub fn retain_codex_session_catalog_paths(
        &self,
        paths: &BTreeSet<String>,
    ) -> Result<(), StoreError> {
        self.registry
            .retain_codex_session_catalog_paths(paths)
            .map_err(map_registry_error)
    }

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
