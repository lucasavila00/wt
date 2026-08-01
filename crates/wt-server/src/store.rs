use crate::schema::{disk_nodes, instances};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use std::cell::RefCell;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{AppSshAccess, Instance, InstanceName, InstanceStatus, SshAccess};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub struct Store {
    connection: RefCell<SqliteConnection>,
}

#[derive(Clone, Debug)]
pub struct StoredInstance {
    pub instance: Instance,
    pub backend_id: String,
    pub head_disk_id: Uuid,
    pub setup_fingerprint: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("instance already exists")]
    Conflict,
    #[error("instance not found")]
    NotFound,
    #[error("database connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),
    #[error("database error: {0}")]
    Database(#[from] DieselError),
    #[error("database migration error: {0}")]
    Migration(String),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

#[derive(Insertable)]
#[diesel(table_name = instances)]
struct NewInstance<'a> {
    id: String,
    owner: &'a str,
    name: &'a str,
    status: String,
    backend_id: &'a str,
    head_disk_id: String,
    source: &'a str,
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
    setup_fingerprint: &'a str,
    ssh_host_keys: &'static str,
    app_ssh_host_keys: &'static str,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = instances)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct InstanceRow {
    id: String,
    owner: String,
    name: String,
    status: String,
    guest_ip: Option<String>,
    last_error: Option<String>,
    backend_id: String,
    head_disk_id: String,
    source: String,
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
    setup_fingerprint: String,
    ssh_user: Option<String>,
    ssh_host: Option<String>,
    ssh_port: Option<i32>,
    ssh_host_keys: String,
    app_ssh_user: Option<String>,
    app_ssh_port: Option<i32>,
    app_ssh_host_keys: String,
}

#[derive(Insertable)]
#[diesel(table_name = disk_nodes)]
struct NewDiskNode {
    id: String,
    parent_id: Option<String>,
    immutable: bool,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = disk_nodes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct DiskNodeRow {
    id: String,
    parent_id: Option<String>,
    immutable: bool,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::InvalidData(format!("create state directory: {error}"))
            })?;
        }
        let path = path
            .to_str()
            .ok_or_else(|| StoreError::InvalidData("database path is not UTF-8".into()))?;
        let mut connection = SqliteConnection::establish(path)?;
        connection.batch_execute(
            "PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
        )?;
        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        Ok(Self {
            connection: RefCell::new(connection),
        })
    }

    pub fn insert(&self, stored: &StoredInstance) -> Result<(), StoreError> {
        self.connection.borrow_mut().transaction(|connection| {
            insert_disk(connection, stored.head_disk_id, None, false)?;
            insert_instance(connection, stored)
        })
    }

    pub fn reserve_fork(
        &self,
        source_id: Uuid,
        expected_source_disk_id: Uuid,
        source_head_disk_id: Uuid,
        fork: &StoredInstance,
    ) -> Result<(), StoreError> {
        self.connection.borrow_mut().transaction(|connection| {
            let changed = diesel::update(
                disk_nodes::table
                    .find(expected_source_disk_id.to_string())
                    .filter(disk_nodes::immutable.eq(false)),
            )
            .set(disk_nodes::immutable.eq(true))
            .execute(connection)?;
            changed_one(changed)?;
            insert_disk(
                connection,
                source_head_disk_id,
                Some(expected_source_disk_id),
                false,
            )?;
            insert_disk(
                connection,
                fork.head_disk_id,
                Some(expected_source_disk_id),
                false,
            )?;
            let changed = diesel::update(
                instances::table
                    .find(source_id.to_string())
                    .filter(instances::head_disk_id.eq(expected_source_disk_id.to_string())),
            )
            .set(instances::head_disk_id.eq(source_head_disk_id.to_string()))
            .execute(connection)?;
            changed_one(changed)?;
            insert_instance(connection, fork)
        })
    }

    pub fn discard_fork(
        &self,
        source_id: Uuid,
        source_disk_id: Uuid,
        source_head_disk_id: Uuid,
        fork_id: Uuid,
        fork_disk_id: Uuid,
        source_pivoted: bool,
    ) -> Result<Vec<Uuid>, StoreError> {
        self.connection.borrow_mut().transaction(|connection| {
            diesel::delete(instances::table.find(fork_id.to_string())).execute(connection)?;
            diesel::delete(disk_nodes::table.find(fork_disk_id.to_string())).execute(connection)?;
            let mut removed = vec![fork_disk_id];
            if !source_pivoted {
                let changed = diesel::update(instances::table.find(source_id.to_string()))
                    .set(instances::head_disk_id.eq(source_disk_id.to_string()))
                    .execute(connection)?;
                changed_one(changed)?;
                diesel::delete(disk_nodes::table.find(source_head_disk_id.to_string()))
                    .execute(connection)?;
                let changed = diesel::update(disk_nodes::table.find(source_disk_id.to_string()))
                    .set(disk_nodes::immutable.eq(false))
                    .execute(connection)?;
                changed_one(changed)?;
                removed.push(source_head_disk_id);
            }
            Ok(removed)
        })
    }

    pub fn garbage_for_delete(&self, id: Uuid) -> Result<Vec<Uuid>, StoreError> {
        garbage_for_delete(&mut self.connection.borrow_mut(), id)
    }

    pub fn get(&self, owner: &str, name: &InstanceName) -> Result<StoredInstance, StoreError> {
        instances::table
            .filter(instances::owner.eq(owner))
            .filter(instances::name.eq(name.as_str()))
            .select(InstanceRow::as_select())
            .first(&mut *self.connection.borrow_mut())
            .optional()?
            .ok_or(StoreError::NotFound)?
            .try_into()
    }

    pub fn list(&self, owner: &str) -> Result<Vec<StoredInstance>, StoreError> {
        instances::table
            .filter(instances::owner.eq(owner))
            .order(instances::name)
            .select(InstanceRow::as_select())
            .load(&mut *self.connection.borrow_mut())?
            .into_iter()
            .map(TryInto::try_into)
            .collect()
    }

    pub fn reconcile_interrupted(&self) -> Result<(), StoreError> {
        diesel::update(
            instances::table.filter(
                instances::status
                    .eq("provisioning")
                    .or(instances::status.eq("destroying")),
            ),
        )
        .set((
            instances::status.eq("error"),
            instances::last_error.eq("operation was interrupted; remove the world and retry"),
        ))
        .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    pub fn mark_setup(&self, id: Uuid, guest_ip: &str, ssh: &SshAccess) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let changed = diesel::update(instances::table.find(id.to_string()))
            .set((
                instances::status.eq(InstanceStatus::Setup.to_string()),
                instances::guest_ip.eq(guest_ip),
                instances::last_error.eq(None::<String>),
                instances::ssh_user.eq(&ssh.user),
                instances::ssh_host.eq(&ssh.host),
                instances::ssh_port.eq(i32::from(ssh.port)),
                instances::ssh_host_keys.eq(host_keys),
            ))
            .execute(&mut *self.connection.borrow_mut())?;
        changed_one(changed)
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
        let changed = diesel::update(instances::table.find(id.to_string()))
            .set((
                instances::status.eq(InstanceStatus::Running.to_string()),
                instances::guest_ip.eq(guest_ip),
                instances::last_error.eq(None::<String>),
                instances::ssh_user.eq(&ssh.user),
                instances::ssh_host.eq(&ssh.host),
                instances::ssh_port.eq(i32::from(ssh.port)),
                instances::ssh_host_keys.eq(host_keys),
                instances::app_ssh_user.eq(&app_ssh.user),
                instances::app_ssh_port.eq(i32::from(app_ssh.port)),
                instances::app_ssh_host_keys.eq(app_host_keys),
            ))
            .execute(&mut *self.connection.borrow_mut())?;
        changed_one(changed)
    }

    pub fn mark_destroying(&self, id: Uuid) -> Result<(), StoreError> {
        self.update_state(id, InstanceStatus::Destroying, None, None)
    }

    pub fn mark_error(&self, id: Uuid, message: &str) -> Result<(), StoreError> {
        self.update_state(id, InstanceStatus::Error, None, Some(message))
    }

    fn update_state(
        &self,
        id: Uuid,
        status: InstanceStatus,
        guest_ip: Option<&str>,
        last_error: Option<&str>,
    ) -> Result<(), StoreError> {
        let target = instances::table.find(id.to_string());
        let changed = if let Some(guest_ip) = guest_ip {
            diesel::update(target)
                .set((
                    instances::status.eq(status.to_string()),
                    instances::guest_ip.eq(guest_ip),
                    instances::last_error.eq(last_error),
                ))
                .execute(&mut *self.connection.borrow_mut())?
        } else {
            diesel::update(target)
                .set((
                    instances::status.eq(status.to_string()),
                    instances::last_error.eq(last_error),
                ))
                .execute(&mut *self.connection.borrow_mut())?
        };
        changed_one(changed)
    }

    pub fn delete(&self, id: Uuid, garbage: &[Uuid]) -> Result<(), StoreError> {
        self.connection.borrow_mut().transaction(|connection| {
            if garbage_for_delete(connection, id)? != garbage {
                return Err(StoreError::InvalidData(
                    "disk graph changed while deleting world".into(),
                ));
            }
            let changed =
                diesel::delete(instances::table.find(id.to_string())).execute(connection)?;
            changed_one(changed)?;
            for disk_id in garbage {
                let changed = diesel::delete(disk_nodes::table.find(disk_id.to_string()))
                    .execute(connection)?;
                changed_one(changed)?;
            }
            Ok(())
        })
    }
}

impl TryFrom<InstanceRow> for StoredInstance {
    type Error = StoreError;

    fn try_from(row: InstanceRow) -> Result<Self, Self::Error> {
        let ssh = match row.ssh_user {
            Some(user) => Some(SshAccess {
                user,
                host: required(row.ssh_host, "ssh_host")?,
                port: to_u16(required(row.ssh_port, "ssh_port")?, "ssh_port")?,
                host_keys: parse_keys(&row.ssh_host_keys)?,
            }),
            None => None,
        };
        let app_ssh = match row.app_ssh_user {
            Some(user) => Some(AppSshAccess {
                user,
                port: to_u16(required(row.app_ssh_port, "app_ssh_port")?, "app_ssh_port")?,
                host_keys: parse_keys(&row.app_ssh_host_keys)?,
            }),
            None => None,
        };
        Ok(Self {
            instance: Instance {
                id: Uuid::parse_str(&row.id)
                    .map_err(|error| StoreError::InvalidData(error.to_string()))?,
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
                source: row.source,
                vcpus: u32::try_from(row.vcpus).map_err(|_| invalid_number("vcpus", row.vcpus))?,
                memory_mib: u64::try_from(row.memory_mib)
                    .map_err(|_| invalid_number("memory_mib", row.memory_mib))?,
                disk_gib: u64::try_from(row.disk_gib)
                    .map_err(|_| invalid_number("disk_gib", row.disk_gib))?,
                ssh,
                app_ssh,
            },
            backend_id: row.backend_id,
            head_disk_id: Uuid::parse_str(&row.head_disk_id)
                .map_err(|error| StoreError::InvalidData(error.to_string()))?,
            setup_fingerprint: row.setup_fingerprint,
        })
    }
}

fn insert_disk(
    connection: &mut SqliteConnection,
    id: Uuid,
    parent_id: Option<Uuid>,
    immutable: bool,
) -> Result<(), StoreError> {
    let row = NewDiskNode {
        id: id.to_string(),
        parent_id: parent_id.map(|id| id.to_string()),
        immutable,
    };
    insert_result(
        diesel::insert_into(disk_nodes::table)
            .values(row)
            .execute(connection),
    )
}

fn insert_instance(
    connection: &mut SqliteConnection,
    stored: &StoredInstance,
) -> Result<(), StoreError> {
    let instance = &stored.instance;
    let row = NewInstance {
        id: instance.id.to_string(),
        owner: &instance.owner,
        name: instance.name.as_str(),
        status: instance.status.to_string(),
        backend_id: &stored.backend_id,
        head_disk_id: stored.head_disk_id.to_string(),
        source: &instance.source,
        vcpus: instance.vcpus.into(),
        memory_mib: to_i64(instance.memory_mib, "memory_mib")?,
        disk_gib: to_i64(instance.disk_gib, "disk_gib")?,
        setup_fingerprint: &stored.setup_fingerprint,
        ssh_host_keys: "[]",
        app_ssh_host_keys: "[]",
    };
    insert_result(
        diesel::insert_into(instances::table)
            .values(row)
            .execute(connection),
    )
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

fn garbage_for_delete(
    connection: &mut SqliteConnection,
    instance_id: Uuid,
) -> Result<Vec<Uuid>, StoreError> {
    let mut current = instances::table
        .find(instance_id.to_string())
        .select(instances::head_disk_id)
        .first::<String>(connection)
        .optional()?
        .ok_or(StoreError::NotFound)?;
    let mut garbage = Vec::new();
    loop {
        let node = disk_nodes::table
            .find(&current)
            .select(DiskNodeRow::as_select())
            .first::<DiskNodeRow>(connection)?;
        if (garbage.is_empty() && node.immutable) || (!garbage.is_empty() && !node.immutable) {
            return Err(StoreError::InvalidData(
                "disk graph mutability invariant is broken".into(),
            ));
        }
        garbage.push(
            Uuid::parse_str(&node.id)
                .map_err(|error| StoreError::InvalidData(error.to_string()))?,
        );
        let Some(parent_id) = node.parent_id else {
            break;
        };
        let other_children = disk_nodes::table
            .filter(disk_nodes::parent_id.eq(&parent_id))
            .filter(disk_nodes::id.ne(&current))
            .count()
            .get_result::<i64>(connection)?;
        let direct_heads = instances::table
            .filter(instances::head_disk_id.eq(&parent_id))
            .count()
            .get_result::<i64>(connection)?;
        if other_children != 0 || direct_heads != 0 {
            break;
        }
        current = parent_id;
    }
    Ok(garbage)
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

fn to_i64(value: u64, field: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| invalid_number(field, value))
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
    fn open_applies_embedded_migrations() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();

        assert!(store.list("owner").unwrap().is_empty());
        let migrations: i64 =
            diesel::sql_query("SELECT COUNT(*) AS count FROM __diesel_schema_migrations")
                .load::<Count>(&mut *store.connection.borrow_mut())
                .unwrap()[0]
                .count;
        assert_eq!(migrations, 1);
    }

    #[derive(QueryableByName)]
    struct Count {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }
}
