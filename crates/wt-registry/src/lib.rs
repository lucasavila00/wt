pub mod schema;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use schema::guests;
use std::cell::RefCell;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub struct Registry {
    connection: RefCell<SqliteConnection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuestKind {
    World,
    Runner,
}

impl GuestKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Runner => "runner",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resources {
    pub vcpus: u64,
    pub memory_mib: u64,
    pub disk_gib: u64,
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
    pub kind: GuestKind,
    pub backend_id: String,
    pub head_disk_id: Uuid,
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
    kind: &'static str,
    backend_id: &'a str,
    head_disk_id: String,
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = guests)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct GuestRow {
    pub id: String,
    pub kind: String,
    pub backend_id: String,
    pub head_disk_id: String,
    pub vcpus: i64,
    pub memory_mib: i64,
    pub disk_gib: i64,
}

#[derive(QueryableByName)]
struct ResourceSum {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    vcpus: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    memory_mib: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    disk_gib: i64,
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
        kind: guest.kind.as_str(),
        backend_id: &guest.backend_id,
        head_disk_id: guest.head_disk_id.to_string(),
        vcpus: to_i64(guest.resources.vcpus, "vcpus")?,
        memory_mib: to_i64(guest.resources.memory_mib, "memory_mib")?,
        disk_gib: to_i64(guest.resources.disk_gib, "disk_gib")?,
    };
    diesel::insert_into(guests::table)
        .values(row)
        .execute(connection)?;
    Ok(())
}

pub fn reserved_resources(connection: &mut SqliteConnection) -> Result<Resources, RegistryError> {
    let sum = diesel::sql_query(
        "SELECT COALESCE(SUM(vcpus), 0) AS vcpus, COALESCE(SUM(memory_mib), 0) AS memory_mib, COALESCE(SUM(disk_gib), 0) AS disk_gib FROM guests",
    )
    .get_result::<ResourceSum>(connection)?;
    Ok(Resources {
        vcpus: to_u64(sum.vcpus, "vcpus")?,
        memory_mib: to_u64(sum.memory_mib, "memory_mib")?,
        disk_gib: to_u64(sum.disk_gib, "disk_gib")?,
    })
}

impl TryFrom<GuestRow> for Guest {
    type Error = RegistryError;

    fn try_from(row: GuestRow) -> Result<Self, Self::Error> {
        let kind = match row.kind.as_str() {
            "world" => GuestKind::World,
            "runner" => GuestKind::Runner,
            value => {
                return Err(RegistryError::InvalidData(format!(
                    "invalid guest kind: {value}"
                )))
            }
        };
        Ok(Self {
            id: Uuid::parse_str(&row.id)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
            kind,
            backend_id: row.backend_id,
            head_disk_id: Uuid::parse_str(&row.head_disk_id)
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
    use crate::schema::disk_nodes;

    #[test]
    fn world_and_runner_share_atomic_capacity() {
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
            head_disk_id: Uuid::new_v4(),
            resources: limit,
        };
        let world = guest(GuestKind::World);
        first
            .immediate_transaction::<_, RegistryError>(|connection| {
                diesel::insert_into(disk_nodes::table)
                    .values((
                        disk_nodes::id.eq(world.head_disk_id.to_string()),
                        disk_nodes::parent_id.eq(None::<String>),
                        disk_nodes::immutable.eq(false),
                    ))
                    .execute(connection)?;
                insert_guest(connection, &world, limit)
            })
            .unwrap();
        let runner = guest(GuestKind::Runner);
        let error = second
            .immediate_transaction::<_, RegistryError>(|connection| {
                insert_guest(connection, &runner, limit)
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
