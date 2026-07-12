use rusqlite::{params, Connection, ErrorCode as SqliteErrorCode, OptionalExtension};
use std::io::Write;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{AppSshAccess, Instance, InstanceName, InstanceStatus, SshAccess};

#[derive(Debug)]
pub struct Store {
    connection: Connection,
    path: std::path::PathBuf,
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
                 app_ssh_user      TEXT,
                 app_ssh_port      INTEGER,
                 app_ssh_host_keys TEXT NOT NULL,
                 UNIQUE(owner, name)
             );
             CREATE TABLE job_log_chunks (
                 instance_id TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
                 byte_offset INTEGER NOT NULL,
                 data BLOB NOT NULL,
                 PRIMARY KEY(instance_id, byte_offset)
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
        Ok(Self {
            connection,
            path: path.to_owned(),
        })
    }

    pub fn reopen(&self) -> Result<Self, StoreError> {
        Self::open(&self.path)
    }

    pub fn insert(&self, stored: &StoredInstance) -> Result<(), StoreError> {
        let instance = &stored.instance;
        let result = self.connection.execute(
            "INSERT INTO instances
             (id, owner, name, status, backend_id, source, ssh_host_keys, app_ssh_host_keys)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]', '[]')",
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
                        ssh_user, ssh_host, ssh_port, ssh_host_keys,
                        app_ssh_user, app_ssh_port, app_ssh_host_keys
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
                    ssh_user, ssh_host, ssh_port, ssh_host_keys,
                    app_ssh_user, app_ssh_port, app_ssh_host_keys
             FROM instances WHERE owner = ?1 ORDER BY name",
        )?;
        let rows = statement.query_map([owner], row_to_instance)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn get_by_id(&self, id: Uuid) -> Result<StoredInstance, StoreError> {
        self.connection
            .query_row(
                "SELECT id, owner, name, status,
                        guest_ip, last_error, backend_id, source,
                        ssh_user, ssh_host, ssh_port, ssh_host_keys,
                        app_ssh_user, app_ssh_port, app_ssh_host_keys
                 FROM instances WHERE id = ?1",
                [id.to_string()],
                row_to_instance,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn transitional(&self) -> Result<Vec<StoredInstance>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, owner, name, status,
                    guest_ip, last_error, backend_id, source,
                    ssh_user, ssh_host, ssh_port, ssh_host_keys,
                    app_ssh_user, app_ssh_port, app_ssh_host_keys
             FROM instances WHERE status IN ('provisioning', 'destroying')",
        )?;
        let rows = statement
            .query_map([], row_to_instance)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)?;
        Ok(rows)
    }

    pub fn log_writer(&self, id: Uuid) -> JobLog<'_> {
        JobLog { store: self, id }
    }

    pub fn append_log(&self, id: Uuid, data: &[u8]) -> Result<(), StoreError> {
        if data.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.unchecked_transaction()?;
        let offset: u64 = transaction.query_row(
            "SELECT COALESCE(MAX(byte_offset + length(data)), 0)
             FROM job_log_chunks WHERE instance_id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO job_log_chunks (instance_id, byte_offset, data)
             VALUES (?1, ?2, ?3)",
            params![id.to_string(), offset, data],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn read_log(
        &self,
        id: Uuid,
        offset: u64,
        limit: usize,
    ) -> Result<(Vec<u8>, u64), StoreError> {
        let length: u64 = self.connection.query_row(
            "SELECT COALESCE(MAX(byte_offset + length(data)), 0)
             FROM job_log_chunks WHERE instance_id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )?;
        let start = offset.min(length);
        let mut statement = self.connection.prepare(
            "SELECT byte_offset, data FROM job_log_chunks
             WHERE instance_id = ?1 AND byte_offset + length(data) > ?2
             ORDER BY byte_offset",
        )?;
        let mut rows = statement.query(params![id.to_string(), start])?;
        let mut output = Vec::with_capacity(limit);
        while output.len() < limit {
            let Some(row) = rows.next()? else { break };
            let chunk_offset: u64 = row.get(0)?;
            let data: Vec<u8> = row.get(1)?;
            let skip = start.saturating_sub(chunk_offset) as usize;
            let available = &data[skip.min(data.len())..];
            let take = available.len().min(limit - output.len());
            output.extend_from_slice(&available[..take]);
        }
        let next_offset = start + output.len() as u64;
        Ok((output, next_offset))
    }

    pub fn finish_running(
        &self,
        id: Uuid,
        guest_ip: &str,
        ssh: &SshAccess,
        app_ssh: &AppSshAccess,
        terminal_log: &[u8],
    ) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        append_log_transaction(&transaction, id, terminal_log)?;
        let host_keys = serde_json::to_string(&ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let app_host_keys = serde_json::to_string(&app_ssh.host_keys)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?;
        let changed = transaction.execute(
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
        transaction.commit()?;
        Ok(())
    }

    pub fn finish_error(
        &self,
        id: Uuid,
        message: &str,
        terminal_log: &[u8],
    ) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        append_log_transaction(&transaction, id, terminal_log)?;
        let changed = transaction.execute(
            "UPDATE instances SET status = ?2, last_error = ?3 WHERE id = ?1",
            params![id.to_string(), InstanceStatus::Error.to_string(), message],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        transaction.commit()?;
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
            ssh: ssh_from_row(row)?,
            app_ssh: app_ssh_from_row(row)?,
        },
        backend_id: row.get(6)?,
    })
}

pub struct JobLog<'a> {
    store: &'a Store,
    id: Uuid,
}

impl Write for JobLog<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.store
            .append_log(self.id, buffer)
            .map_err(std::io::Error::other)?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn append_log_transaction(
    transaction: &rusqlite::Transaction<'_>,
    id: Uuid,
    data: &[u8],
) -> Result<(), StoreError> {
    if data.is_empty() {
        return Ok(());
    }
    let offset: u64 = transaction.query_row(
        "SELECT COALESCE(MAX(byte_offset + length(data)), 0)
         FROM job_log_chunks WHERE instance_id = ?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO job_log_chunks (instance_id, byte_offset, data) VALUES (?1, ?2, ?3)",
        params![id.to_string(), offset, data],
    )?;
    Ok(())
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

fn app_ssh_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<AppSshAccess>> {
    let user: Option<String> = row.get(12)?;
    let Some(user) = user else {
        return Ok(None);
    };
    let keys: String = row.get(14)?;
    let host_keys =
        serde_json::from_str(&keys).map_err(|error| invalid_column(&error.to_string()))?;
    Ok(Some(AppSshAccess {
        user,
        port: row.get(13)?,
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

    fn stored(name: &str) -> StoredInstance {
        let id = Uuid::new_v4();
        StoredInstance {
            instance: Instance {
                id,
                name: InstanceName::parse(name).unwrap(),
                owner: "tester".to_owned(),
                status: InstanceStatus::Provisioning,
                source: "git@example.test:repo.git".to_owned(),
                guest_ip: None,
                last_error: None,
                ssh: None,
                app_ssh: None,
            },
            backend_id: format!("wt-{}", id.simple()),
        }
    }

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
        assert!(!columns.iter().any(|name| name == "job_acknowledged"));
    }

    #[test]
    fn log_chunks_replay_from_bounded_byte_offsets() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let instance = stored("chunked");
        store.insert(&instance).unwrap();
        store.append_log(instance.instance.id, b"abc").unwrap();
        store.append_log(instance.instance.id, b"def").unwrap();

        assert_eq!(
            store.read_log(instance.instance.id, 0, 4).unwrap(),
            (b"abcd".to_vec(), 4)
        );
        assert_eq!(
            store.read_log(instance.instance.id, 4, 64).unwrap(),
            (b"ef".to_vec(), 6)
        );
        assert_eq!(
            store.read_log(instance.instance.id, 99, 64).unwrap(),
            (Vec::new(), 6)
        );
    }

    #[test]
    fn terminal_log_and_state_are_committed_together() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let instance = stored("failed");
        store.insert(&instance).unwrap();
        store
            .append_log(instance.instance.id, b"working\n")
            .unwrap();

        store
            .finish_error(
                instance.instance.id,
                "injected failure",
                b"ERROR: injected failure\n",
            )
            .unwrap();

        let current = store
            .get("tester", &instance.instance.name)
            .unwrap()
            .instance;
        assert_eq!(current.status, InstanceStatus::Error);
        assert_eq!(current.last_error.as_deref(), Some("injected failure"));
        assert_eq!(
            store.read_log(instance.instance.id, 0, 1024).unwrap().0,
            b"working\nERROR: injected failure\n"
        );
        store.delete(instance.instance.id).unwrap();
        assert_eq!(
            store.read_log(instance.instance.id, 0, 1024).unwrap().0,
            Vec::<u8>::new()
        );
    }
}
