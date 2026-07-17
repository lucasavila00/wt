use rusqlite::{params, Connection, ErrorCode as SqliteErrorCode, OptionalExtension};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{AppSshAccess, Instance, InstanceName, InstanceStatus, SshAccess};

#[derive(Debug)]
pub struct Store {
    connection: Connection,
}

#[derive(Clone, Debug)]
pub struct StoredInstance {
    pub instance: Instance,
    pub backend_id: String,
    pub setup_fingerprint: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("instance already exists")]
    Conflict,
    #[error("instance not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let create = !path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::InvalidData(format!("create state directory: {error}"))
            })?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        if create {
            connection.execute_batch(
                "CREATE TABLE instances (
                 id            TEXT PRIMARY KEY,
                 owner         TEXT NOT NULL,
                 name          TEXT NOT NULL,
                 status        TEXT NOT NULL,
                 guest_ip      TEXT,
                 last_error    TEXT,
                 backend_id    TEXT NOT NULL UNIQUE,
                 source        TEXT NOT NULL,
                 vcpus         INTEGER NOT NULL,
                 memory_mib    INTEGER NOT NULL,
                 disk_gib      INTEGER NOT NULL,
                 setup_fingerprint TEXT NOT NULL,
                 ssh_user      TEXT,
                 ssh_host      TEXT,
                 ssh_port      INTEGER,
                 ssh_host_keys TEXT NOT NULL,
                 app_ssh_user      TEXT,
                 app_ssh_port      INTEGER,
                 app_ssh_host_keys TEXT NOT NULL,
                 UNIQUE(owner, name)
             );
             PRAGMA user_version = 1;",
            )?;
        }
        let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != 1 {
            return Err(StoreError::InvalidData(format!(
                "unsupported registry schema version {version}; expected 1; run make clear before reinstalling"
            )));
        }
        Ok(Self { connection })
    }

    pub fn insert(&self, stored: &StoredInstance) -> Result<(), StoreError> {
        let instance = &stored.instance;
        let result = self.connection.execute(
            "INSERT INTO instances
             (id, owner, name, status, backend_id, source,
              vcpus, memory_mib, disk_gib, setup_fingerprint, ssh_host_keys, app_ssh_host_keys)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, '[]', '[]')",
            params![
                instance.id.to_string(),
                instance.owner,
                instance.name.as_str(),
                instance.status.to_string(),
                stored.backend_id,
                instance.source,
                instance.vcpus,
                instance.memory_mib,
                instance.disk_gib,
                stored.setup_fingerprint,
            ],
        );
        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == SqliteErrorCode::ConstraintViolation =>
            {
                Err(StoreError::Conflict)
            }
            Err(error) => Err(StoreError::Database(error)),
        }
    }

    pub fn get(&self, owner: &str, name: &InstanceName) -> Result<StoredInstance, StoreError> {
        self.connection
            .query_row(
                "SELECT id, owner, name, status,
                        guest_ip, last_error, backend_id, source,
                        vcpus, memory_mib, disk_gib,
                        ssh_user, ssh_host, ssh_port, ssh_host_keys,
                        app_ssh_user, app_ssh_port, app_ssh_host_keys, setup_fingerprint
                 FROM instances WHERE owner = ?1 AND name = ?2",
                params![owner, name.as_str()],
                row_to_instance,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn list(&self, owner: &str) -> Result<Vec<StoredInstance>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, owner, name, status,
                    guest_ip, last_error, backend_id, source,
                    vcpus, memory_mib, disk_gib,
                    ssh_user, ssh_host, ssh_port, ssh_host_keys,
                    app_ssh_user, app_ssh_port, app_ssh_host_keys, setup_fingerprint
             FROM instances WHERE owner = ?1 ORDER BY name",
        )?;
        let rows = statement.query_map([owner], row_to_instance)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn reconcile_interrupted(&self) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE instances
             SET status = 'error', last_error = ?1
             WHERE status IN ('provisioning', 'destroying')",
            ["operation was interrupted; remove the world and retry"],
        )?;
        Ok(())
    }

    pub fn mark_setup(&self, id: Uuid, guest_ip: &str, ssh: &SshAccess) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let changed = self.connection.execute(
            "UPDATE instances SET status = ?2, guest_ip = ?3, last_error = NULL,
             ssh_user = ?4, ssh_host = ?5, ssh_port = ?6, ssh_host_keys = ?7 WHERE id = ?1",
            params![
                id.to_string(),
                InstanceStatus::Setup.to_string(),
                guest_ip,
                ssh.user,
                ssh.host,
                ssh.port,
                host_keys,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
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
        let changed = self.connection.execute(
            "UPDATE instances SET status = ?2, guest_ip = ?3, last_error = NULL,
             ssh_user = ?4, ssh_host = ?5, ssh_port = ?6, ssh_host_keys = ?7,
             app_ssh_user = ?8, app_ssh_port = ?9, app_ssh_host_keys = ?10 WHERE id = ?1",
            params![
                id.to_string(),
                InstanceStatus::Running.to_string(),
                guest_ip,
                ssh.user,
                ssh.host,
                ssh.port,
                host_keys,
                app_ssh.user,
                app_ssh.port,
                app_host_keys,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
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
        let changed = self.connection.execute(
            "UPDATE instances
             SET status = ?2,
                 guest_ip = COALESCE(?3, guest_ip),
                 last_error = ?4
             WHERE id = ?1",
            params![id.to_string(), status.to_string(), guest_ip, last_error,],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete(&self, id: Uuid) -> Result<(), StoreError> {
        if self
            .connection
            .execute("DELETE FROM instances WHERE id = ?1", [id.to_string()])?
            == 0
        {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

fn row_to_instance(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredInstance> {
    let id: String = row.get(0)?;
    let name: String = row.get(2)?;
    let status: String = row.get(3)?;
    Ok(StoredInstance {
        instance: Instance {
            id: Uuid::parse_str(&id).map_err(|error| invalid_column(&error.to_string()))?,
            owner: row.get(1)?,
            name: InstanceName::parse(name).map_err(|error| invalid_column(&error.to_string()))?,
            status: status
                .parse()
                .map_err(|error: wt_api::ParseStatusError| invalid_column(&error.to_string()))?,
            guest_ip: row.get(4)?,
            last_error: row.get(5)?,
            source: row.get(7)?,
            vcpus: row.get(8)?,
            memory_mib: row.get(9)?,
            disk_gib: row.get(10)?,
            ssh: ssh_from_row(row)?,
            app_ssh: app_ssh_from_row(row)?,
        },
        backend_id: row.get(6)?,
        setup_fingerprint: row.get(18)?,
    })
}

fn ssh_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<SshAccess>> {
    let user: Option<String> = row.get(11)?;
    let Some(user) = user else {
        return Ok(None);
    };
    let keys: String = row.get(14)?;
    let host_keys =
        serde_json::from_str(&keys).map_err(|error| invalid_column(&error.to_string()))?;
    Ok(Some(SshAccess {
        user,
        host: row.get(12)?,
        port: row.get(13)?,
        host_keys,
    }))
}

fn app_ssh_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<AppSshAccess>> {
    let user: Option<String> = row.get(15)?;
    let Some(user) = user else {
        return Ok(None);
    };
    let keys: String = row.get(17)?;
    let host_keys =
        serde_json::from_str(&keys).map_err(|error| invalid_column(&error.to_string()))?;
    Ok(Some(AppSshAccess {
        user,
        port: row.get(16)?,
        host_keys,
    }))
}

fn invalid_column(message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_owned(),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_registry_uses_daemon_native_schema_version_one() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let version: u32 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let mut statement = store
            .connection
            .prepare("PRAGMA table_info(instances)")
            .unwrap();
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(version, 1);
        assert!(columns.iter().any(|name| name == "setup_fingerprint"));
        for name in ["vcpus", "memory_mib", "disk_gib"] {
            assert!(columns.iter().any(|column| column == name));
        }
        assert!(!columns.iter().any(|name| name == "job_acknowledged"));
        let log_table: Option<String> = store
            .connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'job_log_chunks'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert!(log_table.is_none());
    }
}
