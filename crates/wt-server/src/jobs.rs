use crate::store::{Store, StoreError, StoredInstance};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{GitPassphrase, InstanceStatus};
use wt_libvirt::{ProvisionSpec, WorldWorker};

#[derive(Clone, Debug)]
pub struct Jobs {
    directory: PathBuf,
}

#[derive(Debug)]
pub struct JobLock {
    _file: File,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadLauncher;

pub trait ProvisionLauncher<W> {
    fn launch(
        &self,
        store: &Store,
        worker: &W,
        stored: &StoredInstance,
        passphrase: &GitPassphrase,
        lock: JobLock,
    ) -> Result<(), JobError>;
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

    pub fn reconcile(&self, store: &Store) -> Result<(), StoreError> {
        for stored in store.transitional()? {
            if self
                .is_locked(stored.instance.id)
                .map_err(|error| StoreError::InvalidData(error.to_string()))?
            {
                continue;
            }
            let message = match stored.instance.status {
                InstanceStatus::Provisioning => {
                    "provisioning was interrupted; remove the world with wt rm"
                }
                InstanceStatus::Destroying => "deletion was interrupted; retry wt rm",
                _ => continue,
            };
            store.finish_error(
                stored.instance.id,
                message,
                format!("ERROR: {message}\n").as_bytes(),
            )?;
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

impl<W> ProvisionLauncher<W> for ThreadLauncher
where
    W: WorldWorker + Clone + Send + 'static,
{
    fn launch(
        &self,
        store: &Store,
        worker: &W,
        stored: &StoredInstance,
        passphrase: &GitPassphrase,
        lock: JobLock,
    ) -> Result<(), JobError> {
        let store = store.reopen().map_err(std::io::Error::other)?;
        let worker = worker.clone();
        let stored = stored.clone();
        let passphrase = GitPassphrase::new(passphrase.expose_secret().to_owned());
        std::thread::Builder::new()
            .name(format!("wt-provision-{}", stored.instance.id))
            .spawn(move || {
                let _lock = lock;
                if let Err(error) = run_provision(&store, &worker, stored.clone(), &passphrase) {
                    let message = error.to_string();
                    let _ = store.finish_error(
                        stored.instance.id,
                        &message,
                        format!("ERROR: {message}\n").as_bytes(),
                    );
                }
            })?;
        Ok(())
    }
}

pub fn run_provision<W: WorldWorker>(
    store: &Store,
    worker: &W,
    stored: StoredInstance,
    passphrase: &GitPassphrase,
) -> Result<(), StoreError> {
    let spec = ProvisionSpec {
        id: stored.instance.id,
        backend_id: &stored.backend_id,
        owner: &stored.instance.owner,
        name: &stored.instance.name,
        source: &stored.instance.source,
        git_passphrase: passphrase,
    };
    let mut log = store.log_writer(stored.instance.id);
    match worker.provision(&spec, &mut log) {
        Ok(world) => store.finish_running(
            stored.instance.id,
            &world.guest_ip,
            &world.ssh,
            &world.app_ssh,
            format!("SUCCESS: world {} is running\n", stored.instance.name).as_bytes(),
        ),
        Err(error) => {
            let message = error.to_string();
            store.finish_error(
                stored.instance.id,
                &message,
                format!("ERROR: {message}\n").as_bytes(),
            )
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use wt_api::{Instance, InstanceName};

    fn insert_provisioning(store: &Store, id: Uuid) -> StoredInstance {
        let stored = StoredInstance {
            instance: Instance {
                id,
                name: InstanceName::parse("recovery").unwrap(),
                owner: "tester".to_owned(),
                status: InstanceStatus::Provisioning,
                source: "git@example.test:repo.git".to_owned(),
                guest_ip: None,
                last_error: None,
                ssh: None,
                app_ssh: None,
            },
            backend_id: format!("wt-{}", id.simple()),
        };
        store.insert(&stored).unwrap();
        stored
    }

    #[test]
    fn active_lock_prevents_false_abandonment_then_recovery_records_error() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let jobs = Jobs::open(temp.path().join("jobs")).unwrap();
        let id = Uuid::new_v4();
        let lock = jobs.lock(id).unwrap();
        insert_provisioning(&store, id);

        jobs.reconcile(&store).unwrap();
        assert_eq!(
            store.get_by_id(id).unwrap().instance.status,
            InstanceStatus::Provisioning
        );

        drop(lock);
        jobs.reconcile(&store).unwrap();
        let recovered = store.get_by_id(id).unwrap().instance;
        assert_eq!(recovered.status, InstanceStatus::Error);
        insta::assert_snapshot!(recovered.last_error.unwrap(), @"provisioning was interrupted; remove the world with wt rm");
        insta::assert_snapshot!(
            String::from_utf8(store.read_log(id, 0, 1024).unwrap().0).unwrap(),
            @"ERROR: provisioning was interrupted; remove the world with wt rm"
        );
    }
}
