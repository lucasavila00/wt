use rusqlite::{params, Connection, ErrorCode as SqliteErrorCode, OptionalExtension};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{Instance, InstanceName, InstanceStatus, SshEndpoint};

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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::InvalidData(format!("create state directory: {error}"))
            })?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS instances (
                 id            TEXT PRIMARY KEY,
                 owner         TEXT NOT NULL,
                 name          TEXT NOT NULL,
                 source        TEXT NOT NULL,
                 git_ref       TEXT,
                 status        TEXT NOT NULL,
                 endpoint_user TEXT,
                 endpoint_host TEXT,
                 endpoint_port INTEGER,
                 last_error    TEXT,
                 backend_id    TEXT NOT NULL UNIQUE,
                 UNIQUE(owner, name)
             );",
        )?;
        Ok(Self { connection })
    }

    pub fn insert(&self, stored: &StoredInstance) -> Result<(), StoreError> {
        let instance = &stored.instance;
        let result = self.connection.execute(
            "INSERT INTO instances
             (id, owner, name, source, git_ref, status, backend_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                instance.id.to_string(),
                instance.owner,
                instance.name.as_str(),
                instance.source,
                instance.git_ref,
                instance.status.to_string(),
                stored.backend_id,
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
                "SELECT id, owner, name, source, git_ref, status,
                        endpoint_user, endpoint_host, endpoint_port,
                        last_error, backend_id
                 FROM instances WHERE owner = ?1 AND name = ?2",
                params![owner, name.as_str()],
                row_to_instance,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn list(&self, owner: &str) -> Result<Vec<StoredInstance>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, owner, name, source, git_ref, status,
                    endpoint_user, endpoint_host, endpoint_port,
                    last_error, backend_id
             FROM instances WHERE owner = ?1 ORDER BY name",
        )?;
        let rows = statement.query_map([owner], row_to_instance)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn mark_running(&self, id: Uuid, endpoint: &SshEndpoint) -> Result<(), StoreError> {
        self.update_state(id, InstanceStatus::Running, Some(endpoint), None)
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
        endpoint: Option<&SshEndpoint>,
        last_error: Option<&str>,
    ) -> Result<(), StoreError> {
        let changed = self.connection.execute(
            "UPDATE instances
             SET status = ?2,
                 endpoint_user = COALESCE(?3, endpoint_user),
                 endpoint_host = COALESCE(?4, endpoint_host),
                 endpoint_port = COALESCE(?5, endpoint_port),
                 last_error = ?6
             WHERE id = ?1",
            params![
                id.to_string(),
                status.to_string(),
                endpoint.map(|value| value.user.as_str()),
                endpoint.map(|value| value.host.as_str()),
                endpoint.map(|value| value.port),
                last_error,
            ],
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
    let status: String = row.get(5)?;
    let endpoint_user: Option<String> = row.get(6)?;
    let endpoint_host: Option<String> = row.get(7)?;
    let endpoint_port: Option<u16> = row.get(8)?;
    let endpoint = match (endpoint_user, endpoint_host, endpoint_port) {
        (Some(user), Some(host), Some(port)) => Some(SshEndpoint { user, host, port }),
        (None, None, None) => None,
        _ => return Err(invalid_column("incomplete SSH endpoint")),
    };

    Ok(StoredInstance {
        instance: Instance {
            id: Uuid::parse_str(&id).map_err(|error| invalid_column(&error.to_string()))?,
            owner: row.get(1)?,
            name: InstanceName::parse(name).map_err(|error| invalid_column(&error.to_string()))?,
            source: row.get(3)?,
            git_ref: row.get(4)?,
            status: status
                .parse()
                .map_err(|error: wt_api::ParseStatusError| invalid_column(&error.to_string()))?,
            endpoint,
            last_error: row.get(9)?,
        },
        backend_id: row.get(10)?,
    })
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
