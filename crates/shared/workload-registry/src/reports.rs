use crate::schema::{agent_tool_reports, worlds};
use crate::{Registry, RegistryError};
use diesel::prelude::*;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentToolReportKind {
    Bug,
    Issue,
    Improvement,
    FeatureRequest,
}

impl AgentToolReportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Issue => "issue",
            Self::Improvement => "improvement",
            Self::FeatureRequest => "feature_request",
        }
    }

    fn parse(value: &str) -> Result<Self, RegistryError> {
        match value {
            "bug" => Ok(Self::Bug),
            "issue" => Ok(Self::Issue),
            "improvement" => Ok(Self::Improvement),
            "feature_request" => Ok(Self::FeatureRequest),
            _ => Err(RegistryError::InvalidData(format!(
                "invalid agent report kind: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentToolReport {
    pub world_id: Uuid,
    pub world_name: String,
    pub kind: AgentToolReportKind,
    pub description: String,
}

#[derive(Insertable)]
#[diesel(table_name = agent_tool_reports)]
struct NewAgentToolReport<'a> {
    world_id: String,
    kind: &'static str,
    description: &'a str,
}

#[derive(Queryable)]
struct AgentToolReportRow {
    world_id: String,
    world_name: String,
    kind: String,
    description: String,
}

impl Registry {
    pub fn insert_agent_tool_report(
        &self,
        world_id: Uuid,
        kind: AgentToolReportKind,
        description: &str,
    ) -> Result<(), RegistryError> {
        if description.trim().is_empty() {
            return Err(RegistryError::InvalidData(
                "agent tool report description is empty".into(),
            ));
        }
        self.read(|connection| {
            diesel::insert_into(agent_tool_reports::table)
                .values(NewAgentToolReport {
                    world_id: world_id.to_string(),
                    kind: kind.as_str(),
                    description,
                })
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn list_agent_tool_reports(
        &self,
        owner: &str,
    ) -> Result<Vec<AgentToolReport>, RegistryError> {
        self.read(|connection| {
            agent_tool_reports::table
                .inner_join(worlds::table)
                .filter(worlds::owner.eq(owner))
                .order(agent_tool_reports::id)
                .select((
                    agent_tool_reports::world_id,
                    worlds::name,
                    agent_tool_reports::kind,
                    agent_tool_reports::description,
                ))
                .load::<AgentToolReportRow>(connection)?
                .into_iter()
                .map(|row| {
                    Ok(AgentToolReport {
                        world_id: Uuid::parse_str(&row.world_id)
                            .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
                        world_name: row.world_name,
                        kind: AgentToolReportKind::parse(&row.kind)?,
                        description: row.description,
                    })
                })
                .collect()
        })
    }

    pub fn agent_tool_report_counts(
        &self,
        owner: &str,
    ) -> Result<BTreeMap<Uuid, u64>, RegistryError> {
        let reports = self.list_agent_tool_reports(owner)?;
        let mut counts = BTreeMap::new();
        for report in reports {
            *counts.entry(report.world_id).or_default() += 1;
        }
        Ok(counts)
    }

    pub fn clear_agent_tool_reports(&self, owner: &str) -> Result<u64, RegistryError> {
        self.read(|connection| {
            let world_ids = worlds::table
                .filter(worlds::owner.eq(owner))
                .select(worlds::id);
            let deleted = diesel::delete(
                agent_tool_reports::table.filter(agent_tool_reports::world_id.eq_any(world_ids)),
            )
            .execute(connection)?;
            u64::try_from(deleted)
                .map_err(|_| RegistryError::InvalidData("report count is too large".into()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{disks, guests, worlds};

    #[test]
    fn reports_are_attributed_counted_and_cleared_by_owner() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let first = insert_world(&registry, "alice", "first");
        let second = insert_world(&registry, "bob", "second");

        registry
            .insert_agent_tool_report(first, AgentToolReportKind::Bug, "build is broken")
            .unwrap();
        registry
            .insert_agent_tool_report(first, AgentToolReportKind::FeatureRequest, "add search")
            .unwrap();
        registry
            .insert_agent_tool_report(second, AgentToolReportKind::Issue, "unclear output")
            .unwrap();

        let reports = registry.list_agent_tool_reports("alice").unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].world_id, first);
        assert_eq!(reports[0].world_name, "first");
        assert_eq!(reports[0].kind, AgentToolReportKind::Bug);
        assert_eq!(reports[0].description, "build is broken");
        assert_eq!(
            registry.agent_tool_report_counts("alice").unwrap()[&first],
            2
        );
        assert_eq!(registry.clear_agent_tool_reports("alice").unwrap(), 2);
        assert!(registry
            .list_agent_tool_reports("alice")
            .unwrap()
            .is_empty());
        assert_eq!(registry.list_agent_tool_reports("bob").unwrap().len(), 1);
    }

    #[test]
    fn rejects_empty_report_descriptions() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let error = registry
            .insert_agent_tool_report(Uuid::new_v4(), AgentToolReportKind::Issue, "  \n")
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid stored data: agent tool report description is empty"
        );
    }

    fn insert_world(registry: &Registry, owner: &str, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        let disk_id = Uuid::new_v4();
        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::insert_into(disks::table)
                    .values(disks::id.eq(disk_id.to_string()))
                    .execute(connection)?;
                diesel::insert_into(guests::table)
                    .values((
                        guests::id.eq(id.to_string()),
                        guests::kind.eq("host"),
                        guests::backend_id.eq(format!("wt-{}", id.simple())),
                        guests::disk_id.eq(disk_id.to_string()),
                        guests::vcpus.eq(1_i64),
                        guests::memory_mib.eq(1024_i64),
                        guests::disk_gib.eq(10_i64),
                        guests::compute_reserved.eq(true),
                        guests::disk_reserved_gib.eq(10_i64),
                    ))
                    .execute(connection)?;
                diesel::insert_into(worlds::table)
                    .values((
                        worlds::id.eq(id.to_string()),
                        worlds::owner.eq(owner),
                        worlds::name.eq(name),
                        worlds::status.eq("running"),
                        worlds::setup_fingerprint.eq("fingerprint"),
                        worlds::ssh_host_keys.eq("[]"),
                    ))
                    .execute(connection)?;
                Ok(())
            })
            .unwrap();
        id
    }
}
