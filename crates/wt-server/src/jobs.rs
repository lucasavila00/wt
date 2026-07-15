use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Jobs {
    directory: PathBuf,
}

#[derive(Debug)]
pub struct JobLock {
    _file: File,
}

#[derive(Debug, Error)]
pub enum JobError {
    #[error("job I/O: {0}")]
    Io(#[from] std::io::Error),
}

impl Jobs {
    pub fn open(directory: PathBuf) -> Result<Self, JobError> {
        std::fs::create_dir_all(&directory)?;
        Ok(Self { directory })
    }

    pub fn lock(&self, id: Uuid) -> Result<JobLock, JobError> {
        let file = self.open_lock(id)?;
        file.try_lock().map_err(map_lock_error)?;
        Ok(JobLock { _file: file })
    }

    pub fn wait(&self, id: Uuid) -> Result<JobLock, JobError> {
        let file = self.open_lock(id)?;
        file.lock().map_err(JobError::Io)?;
        Ok(JobLock { _file: file })
    }

    pub fn is_locked(&self, id: Uuid) -> Result<bool, JobError> {
        let file = self.open_lock(id)?;
        match file.try_lock() {
            Ok(()) => Ok(false),
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(error) => Err(map_lock_error(error)),
        }
    }

    pub fn remove(&self, id: Uuid) -> Result<(), JobError> {
        match std::fs::remove_file(self.lock_path(id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn reconcile(&self, store: &crate::store::Store) -> Result<(), crate::store::StoreError> {
        for stored in store.transitional()? {
            if !self
                .is_locked(stored.instance.id)
                .map_err(|error| crate::store::StoreError::InvalidData(error.to_string()))?
            {
                store.mark_error(
                    stored.instance.id,
                    "operation was interrupted; remove the world and retry",
                )?;
            }
        }
        Ok(())
    }

    fn open_lock(&self, id: Uuid) -> Result<File, JobError> {
        Ok(OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.lock_path(id))?)
    }

    fn lock_path(&self, id: Uuid) -> PathBuf {
        self.directory.join(format!("{id}.lock"))
    }
}

fn map_lock_error(error: std::fs::TryLockError) -> JobError {
    match error {
        std::fs::TryLockError::Error(error) => JobError::Io(error),
        std::fs::TryLockError::WouldBlock => JobError::Io(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "job lock is held",
        )),
    }
}
