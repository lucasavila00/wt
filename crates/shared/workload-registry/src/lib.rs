mod capacity;
mod codex_sessions;
mod reports;
pub mod schema;
mod store;

pub use capacity::{
    ensure_resources_reserved, release_resources, reserve_resources, reserved_resources,
};
pub use codex_sessions::{CodexSessionReport, CodexSessionReportInput, CodexSessionState};
pub use reports::{AgentToolReport, AgentToolReportKind};
pub use store::{Store, StoreError, StoredApplication, StoredInstance};

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use schema::{disks, guests};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Guest {
    pub id: Uuid,
    pub backend_id: String,
    pub disk_id: Uuid,
    pub resources: Resources,
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
    #[error("database connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),
    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),
    #[error("database migration error: {0}")]
    Migration(String),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

#[derive(Insertable)]
#[diesel(table_name = guests)]
struct NewGuest<'a> {
    id: String,
    backend_id: &'a str,
    disk_id: String,
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
    compute_reserved: bool,
    disk_reserved_gib: i64,
}

#[derive(Insertable)]
#[diesel(table_name = disks)]
struct NewDiskNode {
    id: String,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = guests)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct GuestRow {
    pub id: String,
    pub backend_id: String,
    pub disk_id: String,
    pub vcpus: i64,
    pub memory_mib: i64,
    pub disk_gib: i64,
    pub compute_reserved: bool,
    pub disk_reserved_gib: i64,
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

pub fn insert_guest(
    connection: &mut SqliteConnection,
    guest: &Guest,
    limit: Resources,
) -> Result<(), RegistryError> {
    if guest.resources.vcpus == 0
        || guest.resources.memory_mib == 0
        || guest.resources.disk_gib == 0
    {
        return Err(RegistryError::ZeroResources);
    }
    let reserved = reserved_resources(connection)?;
    for (resource, reserved, requested, total) in [
        (
            Resource::Memory,
            reserved.memory_mib,
            guest.resources.memory_mib,
            limit.memory_mib,
        ),
        (
            Resource::Cpu,
            reserved.vcpus,
            guest.resources.vcpus,
            limit.vcpus,
        ),
        (
            Resource::Disk,
            reserved.disk_gib,
            guest.resources.disk_gib,
            limit.disk_gib,
        ),
    ] {
        if reserved
            .checked_add(requested)
            .is_none_or(|sum| sum > total)
        {
            return Err(RegistryError::Capacity {
                resource,
                total,
                reserved,
                requested,
            });
        }
    }
    let row = NewGuest {
        id: guest.id.to_string(),
        backend_id: &guest.backend_id,
        disk_id: guest.disk_id.to_string(),
        vcpus: to_i64(guest.resources.vcpus, "vcpus")?,
        memory_mib: to_i64(guest.resources.memory_mib, "memory_mib")?,
        disk_gib: to_i64(guest.resources.disk_gib, "disk_gib")?,
        compute_reserved: true,
        disk_reserved_gib: to_i64(guest.resources.disk_gib, "disk_reserved_gib")?,
    };
    diesel::insert_into(disks::table)
        .values(NewDiskNode {
            id: guest.disk_id.to_string(),
        })
        .execute(connection)?;
    diesel::insert_into(guests::table)
        .values(row)
        .execute(connection)?;
    Ok(())
}

impl TryFrom<GuestRow> for Guest {
    type Error = RegistryError;

    fn try_from(row: GuestRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
            backend_id: row.backend_id,
            disk_id: Uuid::parse_str(&row.disk_id)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
            resources: Resources {
                vcpus: to_u64(row.vcpus, "vcpus")?,
                memory_mib: to_u64(row.memory_mib, "memory_mib")?,
                disk_gib: to_u64(row.disk_gib, "disk_gib")?,
            },
        })
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
    #[test]
    fn all_world_kinds_share_atomic_capacity() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("registry.db");
        let first = Registry::open(&path).unwrap();
        let second = Registry::open(&path).unwrap();
        let limit = Resources {
            vcpus: 2,
            memory_mib: 2048,
            disk_gib: 16,
        };
        let guest = |kind| Guest {
            id: Uuid::new_v4(),
            kind,
            backend_id: format!("wt-{}", Uuid::new_v4().simple()),
            disk_id: Uuid::new_v4(),
            resources: limit,
        };
        let world = guest(GuestKind::Host);
        first
            .immediate_transaction::<_, RegistryError>(|connection| {
                insert_guest(connection, &world, limit)
            })
            .unwrap();
        for kind in [GuestKind::Host, GuestKind::GithubCi] {
            let candidate = guest(kind);
            let error = second
                .immediate_transaction::<_, RegistryError>(|connection| {
                    insert_guest(connection, &candidate, limit)
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
    }

    #[test]
    fn runner_release_removes_its_guest_and_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let id = Uuid::new_v4();
        let disk_id = Uuid::new_v4();
        let resources = Resources {
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
        };
        let runner = registry
            .reserve_runner(
                id,
                "runner-test",
                format!("wt-{}", id.simple()),
                disk_id,
                resources,
                resources,
            )
            .unwrap();
        assert_eq!(runner.status, RunnerStatus::Reserved);
        registry
            .mark_runner(
                id,
                RunnerStatus::CleanupPending,
                Some(42),
                Some("runner exited"),
            )
            .unwrap();
        let listed = registry.list_runners().unwrap();
        assert_eq!(listed[0].status, RunnerStatus::CleanupPending);
        assert_eq!(listed[0].github_runner_id, Some(42));
        assert_eq!(listed[0].last_error.as_deref(), Some("runner exited"));

        registry.release_runner(id).unwrap();

        assert!(registry.list_runners().unwrap().is_empty());
        assert_eq!(
            registry.read(reserved_resources).unwrap(),
            Resources {
                vcpus: 0,
                memory_mib: 0,
                disk_gib: 0,
            }
        );
    }

    #[test]
    fn stopped_guest_reserves_only_its_used_disk_space() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let id = Uuid::new_v4();
        let resources = Resources {
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
        };
        registry
            .reserve_runner(
                id,
                "runner-test",
                format!("wt-{}", id.simple()),
                Uuid::new_v4(),
                resources,
                resources,
            )
            .unwrap();

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
