use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};
use tempfile::TempDir;
use uuid::Uuid;
use wt_api::{CreateInstance, InstanceName, SshAccess};
use wt_devcontainer::{ForkSpec, ProvisionSpec, World, WorldInspection, WorldWorker};
use wt_provider::{ForkError, WorkerError};
use wt_server::operations::Operations;
use wt_server::service::{AgentGitGateway, Service};
use wt_server::store::Store;

#[derive(Clone, Default)]
pub(crate) struct Worker {
    pub(crate) provisions: Arc<AtomicUsize>,
    pub(crate) destroys: Arc<AtomicUsize>,
    pub(crate) inspections: Arc<AtomicUsize>,
    pub(crate) starts: Arc<AtomicUsize>,
    pub(crate) destroyed_disks: Arc<Mutex<Vec<Vec<Uuid>>>>,
    pub(crate) complete: bool,
    pub(crate) provision_gate: Option<Arc<(Mutex<bool>, Condvar)>>,
    pub(crate) missing: bool,
    pub(crate) changed_guest_identity: bool,
    pub(crate) changed_app_identity: bool,
    pub(crate) provision_error: bool,
    pub(crate) stopped: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Gateway;

#[derive(Clone, Default)]
pub(crate) struct UnavailableGateway {
    pub(crate) revocations: Arc<AtomicUsize>,
}

impl AgentGitGateway for Gateway {
    fn reserve(
        &self,
        world_id: Uuid,
        _source: &str,
        _base: &str,
    ) -> Result<wt_devcontainer_git::Grant, String> {
        Ok(wt_devcontainer_git::Grant {
            id: format!("grant-{world_id}"),
            token: format!("token-{world_id}"),
        })
    }

    fn revoke(&self, _grant_id: &str) -> Result<(), String> {
        Ok(())
    }
}

impl AgentGitGateway for UnavailableGateway {
    fn reserve(
        &self,
        world_id: Uuid,
        _source: &str,
        _base: &str,
    ) -> Result<wt_devcontainer_git::Grant, String> {
        Ok(wt_devcontainer_git::Grant {
            id: format!("grant-{world_id}"),
            token: format!("token-{world_id}"),
        })
    }

    fn revoke(&self, _grant_id: &str) -> Result<(), String> {
        self.revocations.fetch_add(1, Ordering::SeqCst);
        Err("gateway unavailable".to_owned())
    }
}

impl WorldWorker for Worker {
    fn provision(
        &self,
        _spec: &ProvisionSpec<'_>,
        _log: &mut dyn std::io::Write,
    ) -> Result<World, WorkerError> {
        self.provisions.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.provision_gate {
            let (ready, wake) = &**gate;
            let mut released = ready.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
        }
        if self.provision_error {
            return Err(WorkerError::new("provision failed"));
        }
        Ok(world(false))
    }

    fn fork(
        &self,
        _spec: &ForkSpec<'_>,
        _log: &mut dyn std::io::Write,
    ) -> Result<World, ForkError> {
        Err(ForkError::before_pivot(WorkerError::new(
            "world forks are unavailable",
        )))
    }

    fn destroy(&self, _backend_id: &str, disk_ids: &[Uuid]) -> Result<(), WorkerError> {
        self.destroys.fetch_add(1, Ordering::SeqCst);
        self.destroyed_disks.lock().unwrap().push(disk_ids.to_vec());
        Ok(())
    }

    fn inspect(&self, _backend_id: &str) -> Result<WorldInspection, WorkerError> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        if self.missing {
            return Ok(WorldInspection::Missing);
        }
        if self.stopped {
            return Ok(WorldInspection::Stopped {
                reason: Some("crashed".into()),
            });
        }
        let mut inspected = world(self.complete);
        if self.changed_guest_identity {
            inspected.ssh.host_keys = vec!["ssh-ed25519 AAAACHANGED guest".into()];
        }
        if self.changed_app_identity {
            inspected.app_ssh.as_mut().unwrap().host_keys =
                vec!["ssh-ed25519 AAAACHANGED app".into()];
        }
        Ok(WorldInspection::Running(inspected))
    }

    fn start(&self, _backend_id: &str) -> Result<World, WorkerError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(world(self.complete))
    }
}

fn world(complete: bool) -> World {
    World {
        guest_ip: "192.0.2.2".into(),
        ssh: SshAccess {
            user: "wt".into(),
            host: "192.0.2.2".into(),
            port: 22,
            host_keys: vec!["ssh-ed25519 AAAATEST guest".into()],
        },
        app_ssh: complete.then(|| wt_api::AppSshAccess {
            user: "vscode".into(),
            port: 2222,
            host_keys: vec!["ssh-ed25519 AAAAAPP app".into()],
        }),
    }
}

pub(crate) fn create(name: &str) -> CreateInstance {
    CreateInstance {
        name: InstanceName::parse(name).unwrap(),
        source: "git@example.test:repo.git".into(),
        git_base: "main".into(),
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        ssh_authorized_keys: vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example".into()],
    }
}

pub(crate) fn service(temp: &TempDir, worker: Worker) -> Service<Worker, Gateway> {
    Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
        Gateway,
        Operations::default(),
        64 * 1024,
    )
}
