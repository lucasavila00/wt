mod mail;
mod reports;

use crate::schema::worlds;
use crate::{Registry, RegistryError, Resources};
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use wt_control_protocol::{SshAccess, World, WorldId, WorldName, WorldStatus};

pub struct Store {
    pub(crate) registry: Registry,
}

#[derive(Clone, Debug)]
pub struct NewWorld {
    pub world_id: WorldId,
    pub owner: String,
    pub name: WorldName,
    pub status: WorldStatus,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    pub setup_fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct StoredWorld {
    pub world: World,
    pub created_at_unix_ms: i64,
    pub setup_fingerprint: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("world name already exists")]
    Conflict,
    #[error("world not found")]
    NotFound,
    #[error("world {resource:?} capacity is full: {reserved} of {total} reserved; requested {requested}")]
    Capacity {
        resource: crate::Resource,
        total: u64,
        reserved: u64,
        requested: u64,
    },
    #[error("database error: {0}")]
    Database(#[from] DieselError),
    #[error("registry error: {0}")]
    Registry(String),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

#[derive(Insertable)]
#[diesel(table_name = worlds)]
struct NewWorldRow<'a> {
    world_id: String,
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
    compute_reserved: bool,
    disk_reserved_gib: i64,
    owner: &'a str,
    name: &'a str,
    status: String,
    setup_fingerprint: &'a str,
    ssh_host_keys: &'static str,
    created_at_unix_ms: i64,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = worlds)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct WorldRow {
    world_id: String,
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
    owner: String,
    name: String,
    status: String,
    guest_ip: Option<String>,
    last_error: Option<String>,
    setup_fingerprint: String,
    ssh_user: Option<String>,
    ssh_host: Option<String>,
    ssh_port: Option<i32>,
    ssh_host_keys: String,
    created_at_unix_ms: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Ok(Self {
            registry: Registry::open(path).map_err(map_registry_error)?,
        })
    }

    pub fn insert(&self, world: &NewWorld) -> Result<(), StoreError> {
        self.registry
            .transaction(|connection| insert_world(connection, world, Resources::UNLIMITED))
    }

    pub fn insert_with_memory_limit(
        &self,
        world: &NewWorld,
        total_mib: u64,
    ) -> Result<(), StoreError> {
        self.registry.immediate_transaction(|connection| {
            insert_world(
                connection,
                world,
                Resources {
                    memory_mib: total_mib,
                    ..Resources::UNLIMITED
                },
            )
        })
    }

    pub fn insert_with_capacity_limit(
        &self,
        world: &NewWorld,
        limit: Resources,
    ) -> Result<(), StoreError> {
        self.registry
            .immediate_transaction(|connection| insert_world(connection, world, limit))
    }

    pub fn reserve_resources(&self, world_id: WorldId, limit: Resources) -> Result<(), StoreError> {
        self.registry.immediate_transaction(|connection| {
            crate::reserve_resources(connection, world_id, limit).map_err(map_registry_error)
        })
    }

    pub fn ensure_resources_reserved(&self, world_id: WorldId) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            crate::ensure_resources_reserved(connection, world_id).map_err(map_registry_error)
        })
    }

    pub fn reserved_resources(&self) -> Result<Resources, StoreError> {
        self.registry
            .read(crate::reserved_resources)
            .map_err(map_registry_error)
    }

    pub fn get_owned_by_name(
        &self,
        owner: &str,
        name: &WorldName,
    ) -> Result<StoredWorld, StoreError> {
        self.registry.read(|connection| {
            worlds::table
                .filter(worlds::owner.eq(owner))
                .filter(worlds::name.eq(name.as_str()))
                .select(WorldRow::as_select())
                .first::<WorldRow>(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?
                .try_into()
        })
    }

    pub fn get_owned_by_id(
        &self,
        owner: &str,
        world_id: WorldId,
    ) -> Result<StoredWorld, StoreError> {
        self.registry.read(|connection| {
            worlds::table
                .find(world_id.to_string())
                .filter(worlds::owner.eq(owner))
                .select(WorldRow::as_select())
                .first::<WorldRow>(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?
                .try_into()
        })
    }

    pub fn list_owned(&self, owner: &str) -> Result<Vec<StoredWorld>, StoreError> {
        self.registry.read(|connection| {
            worlds::table
                .filter(worlds::owner.eq(owner))
                .order((worlds::created_at_unix_ms, worlds::world_id))
                .select(WorldRow::as_select())
                .load::<WorldRow>(connection)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
    }

    pub fn list_git_activity(
        &self,
        owner: &str,
        query: crate::GitActivityQuery,
    ) -> Result<Vec<crate::GitActivity>, StoreError> {
        self.registry
            .list_git_activity(owner, query)
            .map_err(map_registry_error)
    }

    pub fn list_wt_tools_activity(
        &self,
        owner: &str,
        query: crate::WtToolsActivityQuery,
    ) -> Result<Vec<crate::WtToolsActivity>, StoreError> {
        self.registry
            .list_wt_tools_activity(owner, query)
            .map_err(map_registry_error)
    }

    pub fn reconcile_interrupted(&self) -> Result<(), StoreError> {
        self.clear_incomplete_api_mutations()?;
        self.registry.read(|connection| {
            diesel::update(
                worlds::table.filter(
                    worlds::status
                        .eq("provisioning")
                        .or(worlds::status.eq("destroying")),
                ),
            )
            .set((
                worlds::status.eq("error"),
                worlds::last_error.eq("operation was interrupted; remove the world and retry"),
            ))
            .execute(connection)?;
            Ok(())
        })
    }

    pub fn mark_host_running(
        &self,
        world_id: WorldId,
        guest_ip: &str,
        ssh: &SshAccess,
    ) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        self.registry.read(|connection| {
            let changed = diesel::update(worlds::table.find(world_id.to_string()))
                .set((
                    worlds::status.eq(WorldStatus::Running.to_string()),
                    worlds::guest_ip.eq(guest_ip),
                    worlds::last_error.eq(None::<String>),
                    worlds::ssh_user.eq(&ssh.user),
                    worlds::ssh_host.eq(&ssh.host),
                    worlds::ssh_port.eq(i32::from(ssh.port)),
                    worlds::ssh_host_keys.eq(host_keys),
                ))
                .execute(connection)?;
            changed_one(changed)
        })
    }

    pub fn mark_destroying(&self, world_id: WorldId) -> Result<(), StoreError> {
        self.update_state(world_id, WorldStatus::Destroying, None, None)
    }

    pub fn rename(&self, world_id: WorldId, name: &WorldName) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            let changed = diesel::update(worlds::table.find(world_id.to_string()))
                .set(worlds::name.eq(name.as_str()))
                .execute(connection)?;
            changed_one(changed)
        })
    }

    pub fn mark_error(&self, world_id: WorldId, message: &str) -> Result<(), StoreError> {
        self.update_state(world_id, WorldStatus::Error, None, Some(message))
    }

    pub fn mark_stopped(
        &self,
        world_id: WorldId,
        message: &str,
        disk_usage_bytes: u64,
    ) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            let changed = diesel::update(worlds::table.find(world_id.to_string()))
                .set((
                    worlds::status.eq(WorldStatus::Stopped.to_string()),
                    worlds::last_error.eq(message),
                ))
                .execute(connection)?;
            changed_one(changed)?;
            crate::release_resources(connection, world_id, disk_usage_bytes)
                .map_err(map_registry_error)
        })
    }

    fn update_state(
        &self,
        world_id: WorldId,
        status: WorldStatus,
        guest_ip: Option<&str>,
        last_error: Option<&str>,
    ) -> Result<(), StoreError> {
        self.registry.read(|connection| {
            let target = worlds::table.find(world_id.to_string());
            let changed = if let Some(guest_ip) = guest_ip {
                diesel::update(target)
                    .set((
                        worlds::status.eq(status.to_string()),
                        worlds::guest_ip.eq(guest_ip),
                        worlds::last_error.eq(last_error),
                    ))
                    .execute(connection)?
            } else {
                diesel::update(target)
                    .set((
                        worlds::status.eq(status.to_string()),
                        worlds::last_error.eq(last_error),
                    ))
                    .execute(connection)?
            };
            changed_one(changed)
        })
    }

    pub fn delete(&self, world_id: WorldId) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            let changed =
                diesel::delete(worlds::table.find(world_id.to_string())).execute(connection)?;
            changed_one(changed)?;
            Ok(())
        })
    }
}

impl TryFrom<WorldRow> for StoredWorld {
    type Error = StoreError;

    fn try_from(row: WorldRow) -> Result<Self, Self::Error> {
        let world_id = row
            .world_id
            .parse::<WorldId>()
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let _created_at_unix_ms = crate::to_u64(row.created_at_unix_ms, "created_at_unix_ms")
            .map_err(map_registry_error)?;
        let ssh = match row.ssh_user {
            Some(user) => Some(SshAccess {
                user,
                host: required(row.ssh_host, "ssh_host")?,
                port: to_u16(required(row.ssh_port, "ssh_port")?, "ssh_port")?,
                host_keys: parse_keys(&row.ssh_host_keys)?,
            }),
            None => None,
        };
        Ok(Self {
            world: World {
                world_id,
                owner: row.owner,
                name: WorldName::parse(row.name)
                    .map_err(|error| StoreError::InvalidData(error.to_string()))?,
                status: row.status.parse().map_err(
                    |error: wt_control_protocol::ParseWorldStatusError| {
                        StoreError::InvalidData(error.to_string())
                    },
                )?,
                guest_ip: row.guest_ip,
                last_error: row.last_error,
                vcpus: u32::try_from(
                    crate::to_u64(row.vcpus, "vcpus").map_err(map_registry_error)?,
                )
                .map_err(|_| invalid_number("vcpus", row.vcpus))?,
                memory_mib: crate::to_u64(row.memory_mib, "memory_mib")
                    .map_err(map_registry_error)?,
                disk_gib: crate::to_u64(row.disk_gib, "disk_gib").map_err(map_registry_error)?,
                ssh,
            },
            created_at_unix_ms: row.created_at_unix_ms,
            setup_fingerprint: row.setup_fingerprint,
        })
    }
}

fn insert_world(
    connection: &mut SqliteConnection,
    new_world: &NewWorld,
    limit: Resources,
) -> Result<(), StoreError> {
    let created_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StoreError::InvalidData(error.to_string()))?
        .as_millis()
        .try_into()
        .map_err(|_| StoreError::InvalidData("creation time is too large".into()))?;
    let resources = Resources {
        vcpus: new_world.vcpus.into(),
        memory_mib: new_world.memory_mib,
        disk_gib: new_world.disk_gib,
    };
    crate::capacity::ensure_capacity(connection, resources, limit).map_err(map_registry_error)?;
    let row = NewWorldRow {
        world_id: new_world.world_id.to_string(),
        vcpus: crate::to_i64(resources.vcpus, "vcpus").map_err(map_registry_error)?,
        memory_mib: crate::to_i64(resources.memory_mib, "memory_mib")
            .map_err(map_registry_error)?,
        disk_gib: crate::to_i64(resources.disk_gib, "disk_gib").map_err(map_registry_error)?,
        compute_reserved: true,
        disk_reserved_gib: crate::to_i64(resources.disk_gib, "disk_reserved_gib")
            .map_err(map_registry_error)?,
        owner: &new_world.owner,
        name: new_world.name.as_str(),
        status: new_world.status.to_string(),
        setup_fingerprint: &new_world.setup_fingerprint,
        ssh_host_keys: "[]",
        created_at_unix_ms,
    };
    insert_result(
        diesel::insert_into(worlds::table)
            .values(row)
            .execute(connection),
    )?;
    Ok(())
}

fn map_registry_error(error: RegistryError) -> StoreError {
    match error {
        RegistryError::Capacity {
            resource,
            total,
            reserved,
            requested,
        } => StoreError::Capacity {
            resource,
            total,
            reserved,
            requested,
        },
        RegistryError::NotFound => StoreError::NotFound,
        RegistryError::Database(DieselError::DatabaseError(
            DatabaseErrorKind::UniqueViolation,
            _,
        )) => StoreError::Conflict,
        other => StoreError::Registry(other.to_string()),
    }
}

fn insert_result(result: Result<usize, DieselError>) -> Result<(), StoreError> {
    match result {
        Ok(_) => Ok(()),
        Err(DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
            Err(StoreError::Conflict)
        }
        Err(error) => Err(error.into()),
    }
}

fn changed_one(changed: usize) -> Result<(), StoreError> {
    if changed == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, StoreError> {
    value.ok_or_else(|| StoreError::InvalidData(format!("{field} is missing")))
}

fn parse_keys(value: &str) -> Result<Vec<String>, StoreError> {
    serde_json::from_str(value).map_err(|error| StoreError::InvalidData(error.to_string()))
}

fn to_u16(value: i32, field: &str) -> Result<u16, StoreError> {
    u16::try_from(value).map_err(|_| invalid_number(field, value))
}

fn invalid_number(field: &str, value: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData(format!("invalid {field}: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_world(name: &str) -> NewWorld {
        NewWorld {
            world_id: WorldId::new(),
            name: WorldName::parse(name).unwrap(),
            owner: "owner".into(),
            status: WorldStatus::Running,
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
            setup_fingerprint: "fingerprint".into(),
        }
    }

    #[test]
    fn open_applies_shared_registry_migration() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();

        assert!(store.list_owned("owner").unwrap().is_empty());
    }

    #[test]
    fn assigns_creation_time_when_inserting_world() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let world = new_world("created");
        store.insert(&world).unwrap();

        let stored = store.get_owned_by_id("owner", world.world_id).unwrap();

        assert!(stored.created_at_unix_ms > 0);
    }

    #[test]
    fn lists_worlds_in_creation_order_instead_of_name_order() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let first = new_world("w6");
        let second = new_world("w10");
        store.insert(&first).unwrap();
        store.insert(&second).unwrap();
        store.registry.read(|connection| {
            diesel::update(worlds::table.find(first.world_id.to_string()))
                .set(worlds::created_at_unix_ms.eq(1))
                .execute(connection)
                .unwrap();
            diesel::update(worlds::table.find(second.world_id.to_string()))
                .set(worlds::created_at_unix_ms.eq(2))
                .execute(connection)
                .unwrap();
        });

        let names = store
            .list_owned("owner")
            .unwrap()
            .into_iter()
            .map(|stored| stored.world.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(names, ["w6", "w10"]);
    }

    #[test]
    fn world_names_are_unique_across_owners() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("registry.db")).unwrap();
        let first = new_world("shared");
        let mut second = new_world("shared");
        second.owner = "other-owner".into();

        store.insert(&first).unwrap();
        assert!(matches!(store.insert(&second), Err(StoreError::Conflict)));
        assert!(matches!(
            store.get_owned_by_name("other-owner", &second.name),
            Err(StoreError::NotFound)
        ));
        assert_eq!(
            store
                .get_owned_by_id("owner", first.world_id)
                .unwrap()
                .world
                .name,
            first.name
        );
    }

    #[test]
    fn reports_authoritative_resource_reservations() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let world = new_world("world");
        store.insert(&world).unwrap();

        assert_eq!(
            store.reserved_resources().unwrap(),
            Resources {
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
            }
        );

        store
            .mark_stopped(world.world_id, "stopped", 3 * 1024 * 1024 * 1024)
            .unwrap();
        assert_eq!(
            store.reserved_resources().unwrap(),
            Resources {
                vcpus: 0,
                memory_mib: 0,
                disk_gib: 3,
            }
        );
    }
}
