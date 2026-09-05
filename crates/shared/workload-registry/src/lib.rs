mod activity;
mod api;
mod capacity;
mod reports;
pub mod schema;
mod store;

pub use activity::{
    GitActivity, GitActivityInput, GitActivityKind, GitActivityQuery, RepositoryTargetInput,
    WtToolsActivity, WtToolsActivityInput, WtToolsActivityQuery, ACTIVITY_PAGE_SIZE,
};
pub use api::ApiMutationStart;
pub use capacity::{
    ensure_resources_reserved, release_resources, reserve_resources, reserved_resources,
};
pub use reports::{AgentToolReport, AgentToolReportKind};
pub use store::{NewWorld, Store, StoreError, StoredWorld};

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::Path;
use thiserror::Error;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
pub const CAPACITY_CONFIG_PATH: &str = "/etc/wt/capacity.toml";

pub struct Registry {
    connection: RefCell<SqliteConnection>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub vcpus: u64,
    pub memory_mib: u64,
    pub disk_gib: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityConfig {
    pub version: u32,
    pub limits: Resources,
}

impl CapacityConfig {
    pub fn load() -> Result<Self, String> {
        Self::load_from(Path::new(CAPACITY_CONFIG_PATH))
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("read capacity config {}: {error}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .map_err(|error| format!("parse capacity config {}: {error}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported capacity config version {}; expected 1",
                self.version
            ));
        }
        if self.limits.vcpus == 0 || self.limits.memory_mib == 0 || self.limits.disk_gib == 0 {
            return Err("capacity limits must be greater than zero".into());
        }
        Ok(())
    }
}

impl Resources {
    pub const UNLIMITED: Self = Self {
        vcpus: u64::MAX,
        memory_mib: u64::MAX,
        disk_gib: u64::MAX,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Resource {
    Cpu,
    Memory,
    Disk,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("guest capacity values must be greater than zero")]
    ZeroResources,
    #[error("guest capacity value is too large: {0}")]
    InvalidNumber(&'static str),
    #[error("host capacity is full")]
    Capacity {
        resource: Resource,
        total: u64,
        reserved: u64,
        requested: u64,
    },
    #[error("resource not found")]
    NotFound,
    #[error("database connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),
    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),
    #[error("database migration error: {0}")]
    Migration(String),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

impl Registry {
    pub fn open(path: &Path) -> Result<Self, RegistryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RegistryError::InvalidData(format!("create state directory: {error}"))
            })?;
        }
        let path = path
            .to_str()
            .ok_or_else(|| RegistryError::InvalidData("database path is not UTF-8".into()))?;
        let mut connection = SqliteConnection::establish(path)?;
        connection.batch_execute("PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;")?;
        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|error| RegistryError::Migration(error.to_string()))?;
        Ok(Self {
            connection: RefCell::new(connection),
        })
    }

    pub fn read<T>(&self, read: impl FnOnce(&mut SqliteConnection) -> T) -> T {
        read(&mut self.connection.borrow_mut())
    }

    pub fn transaction<T, E>(
        &self,
        transaction: impl FnOnce(&mut SqliteConnection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<diesel::result::Error>,
    {
        self.connection.borrow_mut().transaction(transaction)
    }

    pub fn immediate_transaction<T, E>(
        &self,
        transaction: impl FnOnce(&mut SqliteConnection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<diesel::result::Error>,
    {
        self.connection
            .borrow_mut()
            .immediate_transaction(transaction)
    }
}

fn to_i64(value: u64, field: &'static str) -> Result<i64, RegistryError> {
    i64::try_from(value).map_err(|_| RegistryError::InvalidNumber(field))
}

fn to_u64(value: i64, field: &'static str) -> Result<u64, RegistryError> {
    u64::try_from(value).map_err(|_| RegistryError::InvalidNumber(field))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{agent_tool_reports, worlds};
    use wt_world::WorldId;

    fn insert_world(registry: &Registry, resources: Resources, limit: Resources) -> WorldId {
        let world_id = WorldId::new();
        registry
            .immediate_transaction::<_, RegistryError>(|connection| {
                capacity::ensure_capacity(connection, resources, limit)?;
                diesel::insert_into(worlds::table)
                    .values((
                        worlds::world_id.eq(world_id.to_string()),
                        worlds::vcpus.eq(to_i64(resources.vcpus, "vcpus")?),
                        worlds::memory_mib.eq(to_i64(resources.memory_mib, "memory_mib")?),
                        worlds::disk_gib.eq(to_i64(resources.disk_gib, "disk_gib")?),
                        worlds::compute_reserved.eq(true),
                        worlds::disk_reserved_gib
                            .eq(to_i64(resources.disk_gib, "disk_reserved_gib")?),
                        worlds::owner.eq("owner"),
                        worlds::name.eq(format!("world-{world_id}")),
                        worlds::status.eq("running"),
                        worlds::setup_fingerprint.eq("fingerprint"),
                        worlds::ssh_host_keys.eq("[]"),
                    ))
                    .execute(connection)?;
                Ok(())
            })
            .unwrap();
        world_id
    }

    #[test]
    fn world_capacity_is_atomic() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("registry.db");
        let first = Registry::open(&path).unwrap();
        let second = Registry::open(&path).unwrap();
        let limit = Resources {
            vcpus: 2,
            memory_mib: 2048,
            disk_gib: 16,
        };
        insert_world(&first, limit, limit);
        let error = second
            .immediate_transaction::<_, RegistryError>(|connection| {
                capacity::ensure_capacity(connection, limit, limit)
            })
            .unwrap_err();
        assert!(matches!(
            error,
            RegistryError::Capacity {
                resource: Resource::Memory,
                total: 2048,
                reserved: 2048,
                requested: 2048,
            }
        ));
    }

    #[test]
    fn stopped_world_reserves_only_its_used_disk_space() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let resources = Resources {
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
        };
        let id = insert_world(&registry, resources, resources);

        registry
            .transaction::<_, RegistryError>(|connection| {
                release_resources(connection, id, 1536 * 1024 * 1024)
            })
            .unwrap();
        assert_eq!(
            registry.read(reserved_resources).unwrap(),
            Resources {
                vcpus: 0,
                memory_mib: 0,
                disk_gib: 2,
            }
        );

        registry
            .immediate_transaction::<_, RegistryError>(|connection| {
                reserve_resources(connection, id, resources)
            })
            .unwrap();
        assert_eq!(registry.read(reserved_resources).unwrap(), resources);
    }

    #[test]
    fn fresh_schema_preserves_feedback_on_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("registry.db");
        let mut connection = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        connection
            .batch_execute("PRAGMA foreign_keys = ON;")
            .unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        let world_id = WorldId::new();
        diesel::insert_into(worlds::table)
            .values((
                worlds::world_id.eq(world_id.to_string()),
                worlds::vcpus.eq(1_i64),
                worlds::memory_mib.eq(1024_i64),
                worlds::disk_gib.eq(10_i64),
                worlds::compute_reserved.eq(true),
                worlds::disk_reserved_gib.eq(10_i64),
                worlds::owner.eq("owner"),
                worlds::name.eq("existing"),
                worlds::status.eq("running"),
                worlds::setup_fingerprint.eq("fingerprint"),
                worlds::ssh_host_keys.eq("[]"),
            ))
            .execute(&mut connection)
            .unwrap();
        diesel::insert_into(agent_tool_reports::table)
            .values((
                agent_tool_reports::world_id.eq(world_id.to_string()),
                agent_tool_reports::kind.eq("bug"),
                agent_tool_reports::description.eq("keep me"),
            ))
            .execute(&mut connection)
            .unwrap();
        drop(connection);

        let registry = Registry::open(&path).unwrap();
        let reports = registry.list_agent_tool_reports("owner").unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].description, "keep me");
    }

    #[test]
    fn capacity_config_is_strict() {
        let config: CapacityConfig = toml::from_str(
            r#"
version = 1

[limits]
vcpus = 16
memory_mib = 32768
disk_gib = 512
"#,
        )
        .unwrap();
        config.validate().unwrap();
        assert_eq!(config.limits.memory_mib, 32768);
        assert!(toml::from_str::<CapacityConfig>(
            r#"
version = 1
unknown = true
[limits]
vcpus = 16
memory_mib = 32768
disk_gib = 512
"#
        )
        .is_err());
    }
}
