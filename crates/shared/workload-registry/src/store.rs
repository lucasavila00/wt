mod reports;

use crate::schema::worlds;
use crate::{Registry, RegistryError, Resources};
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;
use wt_control_protocol::{Instance, InstanceName, InstanceStatus, SshAccess};

pub struct Store {
    registry: Registry,
}

#[derive(Clone, Debug)]
pub struct StoredInstance {
    pub instance: Instance,
    pub backend_id: String,
    pub disk_id: Uuid,
    pub setup_fingerprint: String,
    pub gateway_grant_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("instance already exists")]
    Conflict,
    #[error("instance not found")]
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
struct NewWorld<'a> {
    id: String,
    backend_id: &'a str,
    disk_id: String,
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
    gateway_grant_id: Option<&'a str>,
    created_at_unix_ms: i64,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = worlds)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct WorldRow {
    id: String,
    backend_id: String,
    disk_id: String,
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
    gateway_grant_id: Option<String>,
    created_at_unix_ms: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Ok(Self {
            registry: Registry::open(path).map_err(map_registry_error)?,
        })
    }

    pub fn insert(&self, stored: &StoredInstance) -> Result<(), StoreError> {
        self.registry
            .transaction(|connection| insert_world(connection, stored, Resources::UNLIMITED))
    }

    pub fn insert_with_memory_limit(
        &self,
        stored: &StoredInstance,
        total_mib: u64,
    ) -> Result<(), StoreError> {
        self.registry.immediate_transaction(|connection| {
            insert_world(
                connection,
                stored,
                Resources {
                    memory_mib: total_mib,
                    ..Resources::UNLIMITED
                },
            )
        })
    }

    pub fn insert_with_capacity_limit(
        &self,
        stored: &StoredInstance,
        limit: Resources,
    ) -> Result<(), StoreError> {
        self.registry
            .immediate_transaction(|connection| insert_world(connection, stored, limit))
    }

    pub fn reserve_resources(&self, id: Uuid, limit: Resources) -> Result<(), StoreError> {
        self.registry.immediate_transaction(|connection| {
            crate::reserve_resources(connection, id, limit).map_err(map_registry_error)
        })
    }

    pub fn ensure_resources_reserved(&self, id: Uuid) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            crate::ensure_resources_reserved(connection, id).map_err(map_registry_error)
        })
    }

    pub fn reserved_resources(&self) -> Result<Resources, StoreError> {
        self.registry
            .read(crate::reserved_resources)
            .map_err(map_registry_error)
    }

    pub fn get(&self, owner: &str, name: &InstanceName) -> Result<StoredInstance, StoreError> {
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

    pub fn list(&self, owner: &str) -> Result<Vec<StoredInstance>, StoreError> {
        self.registry.read(|connection| {
            worlds::table
                .filter(worlds::owner.eq(owner))
                .order((worlds::created_at_unix_ms, worlds::id))
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

    pub fn repository_git_state(
        &self,
        owner: &str,
        provider_host: &str,
        repository: &str,
        git_before_id: Option<u64>,
        wt_tools_before_id: Option<u64>,
    ) -> Result<Option<crate::RepositoryGitState>, StoreError> {
        self.registry
            .repository_git_state(
                owner,
                provider_host,
                repository,
                git_before_id,
                wt_tools_before_id,
            )
            .map_err(map_registry_error)
    }

    pub fn reconcile_interrupted(&self) -> Result<(), StoreError> {
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
        id: Uuid,
        guest_ip: &str,
        ssh: &SshAccess,
    ) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        self.registry.read(|connection| {
            let changed = diesel::update(worlds::table.find(id.to_string()))
                .set((
                    worlds::status.eq(InstanceStatus::Running.to_string()),
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

    pub fn mark_destroying(&self, id: Uuid) -> Result<(), StoreError> {
        self.update_state(id, InstanceStatus::Destroying, None, None)
    }

    pub fn mark_error(&self, id: Uuid, message: &str) -> Result<(), StoreError> {
        self.update_state(id, InstanceStatus::Error, None, Some(message))
    }

    pub fn mark_stopped(
        &self,
        id: Uuid,
        message: &str,
        disk_usage_bytes: u64,
    ) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            let changed = diesel::update(worlds::table.find(id.to_string()))
                .set((
                    worlds::status.eq(InstanceStatus::Stopped.to_string()),
                    worlds::last_error.eq(message),
                ))
                .execute(connection)?;
            changed_one(changed)?;
            crate::release_resources(connection, id, disk_usage_bytes).map_err(map_registry_error)
        })
    }

    fn update_state(
        &self,
        id: Uuid,
        status: InstanceStatus,
        guest_ip: Option<&str>,
        last_error: Option<&str>,
    ) -> Result<(), StoreError> {
        self.registry.read(|connection| {
            let target = worlds::table.find(id.to_string());
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

    pub fn delete(&self, id: Uuid, disk_id: Uuid) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            let changed = diesel::delete(
                worlds::table
                    .find(id.to_string())
                    .filter(worlds::disk_id.eq(disk_id.to_string())),
            )
            .execute(connection)?;
            changed_one(changed)?;
            Ok(())
        })
    }
}

impl TryFrom<WorldRow> for StoredInstance {
    type Error = StoreError;

    fn try_from(row: WorldRow) -> Result<Self, Self::Error> {
        let id =
            Uuid::parse_str(&row.id).map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let disk_id = Uuid::parse_str(&row.disk_id)
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
            instance: Instance {
                id,
                owner: row.owner,
                name: InstanceName::parse(row.name)
                    .map_err(|error| StoreError::InvalidData(error.to_string()))?,
                status: row.status.parse().map_err(
                    |error: wt_control_protocol::ParseStatusError| {
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
            backend_id: row.backend_id,
            disk_id,
            setup_fingerprint: row.setup_fingerprint,
            gateway_grant_id: row.gateway_grant_id,
        })
    }
}

fn insert_world(
    connection: &mut SqliteConnection,
    stored: &StoredInstance,
    limit: Resources,
) -> Result<(), StoreError> {
    let instance = &stored.instance;
    let created_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StoreError::InvalidData(error.to_string()))?
        .as_millis()
        .try_into()
        .map_err(|_| StoreError::InvalidData("creation time is too large".into()))?;
    let resources = Resources {
        vcpus: instance.vcpus.into(),
        memory_mib: instance.memory_mib,
        disk_gib: instance.disk_gib,
    };
    crate::capacity::ensure_capacity(connection, resources, limit).map_err(map_registry_error)?;
    let row = NewWorld {
        id: instance.id.to_string(),
        backend_id: &stored.backend_id,
        disk_id: stored.disk_id.to_string(),
        vcpus: crate::to_i64(resources.vcpus, "vcpus").map_err(map_registry_error)?,
        memory_mib: crate::to_i64(resources.memory_mib, "memory_mib")
            .map_err(map_registry_error)?,
        disk_gib: crate::to_i64(resources.disk_gib, "disk_gib").map_err(map_registry_error)?,
        compute_reserved: true,
        disk_reserved_gib: crate::to_i64(resources.disk_gib, "disk_reserved_gib")
            .map_err(map_registry_error)?,
        owner: &instance.owner,
        name: instance.name.as_str(),
        status: instance.status.to_string(),
        setup_fingerprint: &stored.setup_fingerprint,
        ssh_host_keys: "[]",
        gateway_grant_id: stored.gateway_grant_id.as_deref(),
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

    fn stored(name: &str) -> StoredInstance {
        StoredInstance {
            instance: Instance {
                id: Uuid::new_v4(),
                name: InstanceName::parse(name).unwrap(),
                owner: "owner".into(),
                status: InstanceStatus::Running,
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
                guest_ip: None,
                last_error: None,
                ssh: None,
            },
            backend_id: format!("backend-{name}"),
            disk_id: Uuid::new_v4(),
            setup_fingerprint: "fingerprint".into(),
            gateway_grant_id: None,
        }
    }

    #[test]
    fn open_applies_shared_registry_migration() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();

        assert!(store.list("owner").unwrap().is_empty());
    }

    #[test]
    fn lists_worlds_in_creation_order_instead_of_name_order() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let first = stored("w6");
        let second = stored("w10");
        store.insert(&first).unwrap();
        store.insert(&second).unwrap();
        store.registry.read(|connection| {
            diesel::update(worlds::table.find(first.instance.id.to_string()))
                .set(worlds::created_at_unix_ms.eq(1))
                .execute(connection)
                .unwrap();
            diesel::update(worlds::table.find(second.instance.id.to_string()))
                .set(worlds::created_at_unix_ms.eq(2))
                .execute(connection)
                .unwrap();
        });

        let names = store
            .list("owner")
            .unwrap()
            .into_iter()
            .map(|stored| stored.instance.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(names, ["w6", "w10"]);
    }

    #[test]
    fn reports_authoritative_resource_reservations() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let world = stored("world");
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
            .mark_stopped(world.instance.id, "stopped", 3 * 1024 * 1024 * 1024)
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
