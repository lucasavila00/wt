use rusqlite::{params, Connection, ErrorCode as SqliteErrorCode, OptionalExtension};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{Instance, InstanceName, InstanceStatus, SshAccess};

#[derive(Debug)]
pub struct Store {
    connection: Connection,
}

#[derive(Clone, Debug)]
pub struct StoredInstance {
    pub instance: Instance,
    pub backend_id: String,
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
                 ssh_user      TEXT,
                 ssh_host      TEXT,
                 ssh_port      INTEGER,
                 ssh_host_keys TEXT NOT NULL,
                 UNIQUE(owner, name)
             );
             PRAGMA user_version = 1;",
            )?;
        }
        let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != 1 {
            return Err(StoreError::InvalidData(format!(
                "unsupported registry schema version {version}; expected 1"
            )));
        }
        Ok(Self { connection })
    }

    pub fn insert(&self, stored: &StoredInstance) -> Result<(), StoreError> {
        let instance = &stored.instance;
        let result = self.connection.execute(
            "INSERT INTO instances
             (id, owner, name, status, backend_id, source, ssh_host_keys)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]')",
            params![
                instance.id.to_string(),
                instance.owner,
                instance.name.as_str(),
                instance.status.to_string(),
                stored.backend_id,
                instance.source,
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
                        ssh_user, ssh_host, ssh_port, ssh_host_keys
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
                    ssh_user, ssh_host, ssh_port, ssh_host_keys
             FROM instances WHERE owner = ?1 ORDER BY name",
        )?;
        let rows = statement.query_map([owner], row_to_instance)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn mark_running(
        &self,
        id: Uuid,
        guest_ip: &str,
        ssh: &SshAccess,
    ) -> Result<(), StoreError> {
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let changed = self.connection.execute(
            "UPDATE instances SET status = ?2, guest_ip = ?3, last_error = NULL,
             ssh_user = ?4, ssh_host = ?5, ssh_port = ?6, ssh_host_keys = ?7 WHERE id = ?1",
            params![
                id.to_string(),
                InstanceStatus::Running.to_string(),
                guest_ip,
                ssh.user,
                ssh.host,
                ssh.port,
                host_keys
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
            ssh: ssh_from_row(row)?,
        },
        backend_id: row.get(6)?,
    })
}

fn ssh_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<SshAccess>> {
    let user: Option<String> = row.get(8)?;
    let Some(user) = user else {
        return Ok(None);
    };
    let keys: String = row.get(11)?;
    let host_keys =
        serde_json::from_str(&keys).map_err(|error| invalid_column(&error.to_string()))?;
    Ok(Some(SshAccess {
        user,
        host: row.get(9)?,
        port: row.get(10)?,
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
