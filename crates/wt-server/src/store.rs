mod reports;

use crate::schema::{devcontainers, disks, guests, hosts, worlds};
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{
    AppSshAccess, Instance, InstanceApplication, InstanceName, InstanceStatus, SshAccess, WorldKind,
};
use wt_registry::{Guest, GuestKind, GuestRow, Registry, RegistryError, Resources};

pub struct Store {
    registry: Registry,
}

#[derive(Clone, Debug)]
pub struct StoredInstance {
    pub instance: Instance,
    pub backend_id: String,
    pub disk_id: Uuid,
    pub setup_fingerprint: String,
    pub application: StoredApplication,
}

#[derive(Clone, Debug)]
pub enum StoredApplication {
    Devcontainer { gateway_grant_id: String },
    Host { gateway_grant_id: Option<String> },
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("instance already exists")]
    Conflict,
    #[error("instance not found")]
    NotFound,
    #[error("world {resource:?} capacity is full: {reserved} of {total} reserved; requested {requested}")]
    Capacity {
        resource: wt_registry::Resource,
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
    owner: &'a str,
    name: &'a str,
    status: String,
    setup_fingerprint: &'a str,
    ssh_host_keys: &'static str,
}

#[derive(Insertable)]
#[diesel(table_name = devcontainers)]
struct NewDevcontainer<'a> {
    id: String,
    source: &'a str,
    git_base: &'a str,
    git_prefix: &'a str,
    gateway_grant_id: &'a str,
    app_ssh_host_keys: &'static str,
}

#[derive(Insertable)]
#[diesel(table_name = hosts)]
struct NewHost<'a> {
    id: String,
    gateway_grant_id: Option<&'a str>,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = worlds)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct WorldRow {
    id: String,
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
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = devcontainers)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct DevcontainerRow {
    id: String,
    source: String,
    git_base: String,
    git_prefix: String,
    gateway_grant_id: String,
    app_ssh_user: Option<String>,
    app_ssh_port: Option<i32>,
    app_ssh_host_keys: String,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = hosts)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct HostRow {
    id: String,
    gateway_grant_id: Option<String>,
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
            wt_registry::reserve_resources(connection, id, limit).map_err(map_registry_error)
        })
    }

    pub fn ensure_resources_reserved(&self, id: Uuid) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            wt_registry::ensure_resources_reserved(connection, id).map_err(map_registry_error)
        })
    }

    pub fn get(&self, owner: &str, name: &InstanceName) -> Result<StoredInstance, StoreError> {
        self.registry.read(|connection| {
            guests::table
                .inner_join(worlds::table)
                .left_outer_join(devcontainers::table.on(devcontainers::id.eq(worlds::id)))
                .left_outer_join(hosts::table.on(hosts::id.eq(worlds::id)))
                .filter(worlds::owner.eq(owner))
                .filter(worlds::name.eq(name.as_str()))
                .select((
                    GuestRow::as_select(),
                    WorldRow::as_select(),
                    Option::<DevcontainerRow>::as_select(),
                    Option::<HostRow>::as_select(),
                ))
                .first::<(GuestRow, WorldRow, Option<DevcontainerRow>, Option<HostRow>)>(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?
                .try_into()
        })
    }

    pub fn list(&self, owner: &str) -> Result<Vec<StoredInstance>, StoreError> {
        self.registry.read(|connection| {
            guests::table
                .inner_join(worlds::table)
                .left_outer_join(devcontainers::table.on(devcontainers::id.eq(worlds::id)))
                .left_outer_join(hosts::table.on(hosts::id.eq(worlds::id)))
                .filter(worlds::owner.eq(owner))
                .order(worlds::name)
                .select((
                    GuestRow::as_select(),
                    WorldRow::as_select(),
                    Option::<DevcontainerRow>::as_select(),
                    Option::<HostRow>::as_select(),
                ))
                .load::<(GuestRow, WorldRow, Option<DevcontainerRow>, Option<HostRow>)>(connection)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
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

    pub fn mark_setup(&self, id: Uuid, guest_ip: &str, ssh: &SshAccess) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        self.registry.read(|connection| {
            let changed = diesel::update(worlds::table.find(id.to_string()))
                .set((
                    worlds::status.eq(InstanceStatus::Setup.to_string()),
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

    pub fn mark_running(
        &self,
        id: Uuid,
        guest_ip: &str,
        ssh: &SshAccess,
        app_ssh: &AppSshAccess,
    ) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let app_host_keys = serde_json::to_string(&app_ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        self.registry.transaction(|connection| {
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
            changed_one(changed)?;
            let changed = diesel::update(devcontainers::table.find(id.to_string()))
                .set((
                    devcontainers::app_ssh_user.eq(&app_ssh.user),
                    devcontainers::app_ssh_port.eq(i32::from(app_ssh.port)),
                    devcontainers::app_ssh_host_keys.eq(app_host_keys),
                ))
                .execute(connection)?;
            changed_one(changed)
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
            wt_registry::release_resources(connection, id, disk_usage_bytes)
                .map_err(map_registry_error)
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
                guests::table
                    .find(id.to_string())
                    .filter(guests::disk_id.eq(disk_id.to_string())),
            )
            .execute(connection)?;
            changed_one(changed)?;
            let changed =
                diesel::delete(disks::table.find(disk_id.to_string())).execute(connection)?;
            changed_one(changed)?;
            Ok(())
        })
    }
}

impl TryFrom<(GuestRow, WorldRow, Option<DevcontainerRow>, Option<HostRow>)> for StoredInstance {
    type Error = StoreError;

    fn try_from(
        (guest_row, row, devcontainer, host): (
            GuestRow,
            WorldRow,
            Option<DevcontainerRow>,
            Option<HostRow>,
        ),
    ) -> Result<Self, Self::Error> {
        let guest: Guest = guest_row.try_into().map_err(map_registry_error)?;
        if guest.id.to_string() != row.id {
            return Err(StoreError::InvalidData(
                "world is linked to an invalid guest".into(),
            ));
        }
        let ssh = match row.ssh_user {
            Some(user) => Some(SshAccess {
                user,
                host: required(row.ssh_host, "ssh_host")?,
                port: to_u16(required(row.ssh_port, "ssh_port")?, "ssh_port")?,
                host_keys: parse_keys(&row.ssh_host_keys)?,
            }),
            None => None,
        };
        let (application, stored_application) = match (guest.kind, devcontainer, host) {
            (GuestKind::Devcontainer, Some(row), None) if row.id == guest.id.to_string() => {
                let app_ssh = match row.app_ssh_user {
                    Some(user) => Some(AppSshAccess {
                        user,
                        port: to_u16(required(row.app_ssh_port, "app_ssh_port")?, "app_ssh_port")?,
                        host_keys: parse_keys(&row.app_ssh_host_keys)?,
                    }),
                    None => None,
                };
                (
                    InstanceApplication::Devcontainer {
                        source: row.source,
                        git_base: row.git_base,
                        git_prefix: row.git_prefix,
                        app_ssh,
                    },
                    StoredApplication::Devcontainer {
                        gateway_grant_id: row.gateway_grant_id,
                    },
                )
            }
            (GuestKind::Host, None, Some(row)) if row.id == guest.id.to_string() => (
                InstanceApplication::Host,
                StoredApplication::Host {
                    gateway_grant_id: row.gateway_grant_id,
                },
            ),
            _ => {
                return Err(StoreError::InvalidData(
                    "world kind and application record do not match".into(),
                ))
            }
        };
        Ok(Self {
            instance: Instance {
                id: guest.id,
                owner: row.owner,
                name: InstanceName::parse(row.name)
                    .map_err(|error| StoreError::InvalidData(error.to_string()))?,
                status: row
                    .status
                    .parse()
                    .map_err(|error: wt_api::ParseStatusError| {
                        StoreError::InvalidData(error.to_string())
                    })?,
                guest_ip: row.guest_ip,
                last_error: row.last_error,
                vcpus: u32::try_from(guest.resources.vcpus)
                    .map_err(|_| invalid_number("vcpus", guest.resources.vcpus))?,
                memory_mib: guest.resources.memory_mib,
                disk_gib: guest.resources.disk_gib,
                ssh,
                application,
            },
            backend_id: guest.backend_id,
            disk_id: guest.disk_id,
            setup_fingerprint: row.setup_fingerprint,
            application: stored_application,
        })
    }
}

fn insert_world(
    connection: &mut SqliteConnection,
    stored: &StoredInstance,
    limit: Resources,
) -> Result<(), StoreError> {
    let instance = &stored.instance;
    wt_registry::insert_guest(
        connection,
        &Guest {
            id: instance.id,
            kind: match instance.kind() {
                WorldKind::Devcontainer => GuestKind::Devcontainer,
                WorldKind::Host => GuestKind::Host,
                WorldKind::GithubCi => {
                    return Err(StoreError::InvalidData(
                        "github-ci worlds are not retained by wt-server".into(),
                    ))
                }
            },
            backend_id: stored.backend_id.clone(),
            disk_id: stored.disk_id,
            resources: Resources {
                vcpus: instance.vcpus.into(),
                memory_mib: instance.memory_mib,
                disk_gib: instance.disk_gib,
            },
        },
        limit,
    )
    .map_err(map_registry_error)?;
    let row = NewWorld {
        id: instance.id.to_string(),
        owner: &instance.owner,
        name: instance.name.as_str(),
        status: instance.status.to_string(),
        setup_fingerprint: &stored.setup_fingerprint,
        ssh_host_keys: "[]",
    };
    insert_result(
        diesel::insert_into(worlds::table)
            .values(row)
            .execute(connection),
    )?;
    match (&instance.application, &stored.application) {
        (
            InstanceApplication::Devcontainer {
                source,
                git_base,
                git_prefix,
                ..
            },
            StoredApplication::Devcontainer { gateway_grant_id },
        ) => insert_result(
            diesel::insert_into(devcontainers::table)
                .values(NewDevcontainer {
                    id: instance.id.to_string(),
                    source,
                    git_base,
                    git_prefix,
                    gateway_grant_id,
                    app_ssh_host_keys: "[]",
                })
                .execute(connection),
        ),
        (InstanceApplication::Host, StoredApplication::Host { gateway_grant_id }) => insert_result(
            diesel::insert_into(hosts::table)
                .values(NewHost {
                    id: instance.id.to_string(),
                    gateway_grant_id: gateway_grant_id.as_deref(),
                })
                .execute(connection),
        ),
        _ => Err(StoreError::InvalidData(
            "instance and stored application kinds do not match".into(),
        )),
    }
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

    #[test]
    fn open_applies_shared_registry_migration() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();

        assert!(store.list("owner").unwrap().is_empty());
    }
}
