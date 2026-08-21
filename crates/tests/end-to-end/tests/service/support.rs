use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};
use tempfile::TempDir;
use uuid::Uuid;
use wt_control_protocol::{CreateApplication, CreateInstance, InstanceName, WorldKind};
use wt_libvirt_kvm::WorkerError;
use wt_server::operations::Operations;
use wt_server::service::{AgentGitGateway, Service};
use wt_workload_registry::Store;
use wt_retained_worlds::{
    GuestAccess, ProvisionSpec, World, WorldApplication, WorldInspection, WorldWorker,
};

#[derive(Clone, Default)]
pub(crate) struct Worker {
    pub(crate) provisions: Arc<AtomicUsize>,
    pub(crate) destroys: Arc<AtomicUsize>,
    pub(crate) inspections: Arc<AtomicUsize>,
    pub(crate) starts: Arc<AtomicUsize>,
    pub(crate) stops: Arc<AtomicUsize>,
    pub(crate) destroyed_disks: Arc<Mutex<Vec<Vec<Uuid>>>>,
    pub(crate) host_user_data: Arc<Mutex<Vec<String>>>,
    pub(crate) host_git_grants: Arc<Mutex<Vec<String>>>,
    pub(crate) complete: bool,
    pub(crate) provision_gate: Option<Arc<(Mutex<bool>, Condvar)>>,
    pub(crate) missing: bool,
    pub(crate) changed_guest_identity: bool,
    pub(crate) changed_app_identity: bool,
    pub(crate) provision_error: bool,
    pub(crate) host_setup_error: bool,
    pub(crate) stopped: bool,
    pub(crate) is_stopped: Arc<AtomicBool>,
    pub(crate) stop_error: bool,
    pub(crate) disk_usage_bytes: u64,
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
        _source: Option<&str>,
        _base: Option<&str>,
    ) -> Result<wt_agent_git_gateway::Grant, String> {
        Ok(wt_agent_git_gateway::Grant {
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
        _source: Option<&str>,
        _base: Option<&str>,
    ) -> Result<wt_agent_git_gateway::Grant, String> {
        Ok(wt_agent_git_gateway::Grant {
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
        spec: ProvisionSpec<'_>,
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
        let kind = match spec {
            ProvisionSpec::Devcontainer(_) => WorldKind::Devcontainer,
            ProvisionSpec::Host(spec) => {
                self.host_user_data
                    .lock()
                    .unwrap()
                    .push(spec.user_data.to_owned());
                self.host_git_grants
                    .lock()
                    .unwrap()
                    .push(spec.git_grant.to_owned());
                WorldKind::Host
            }
        };
        Ok(world(kind, false))
    }

    fn destroy(
        &self,
        _kind: WorldKind,
        _backend_id: &str,
        disk_id: Uuid,
    ) -> Result<(), WorkerError> {
        self.destroys.fetch_add(1, Ordering::SeqCst);
        self.destroyed_disks.lock().unwrap().push(vec![disk_id]);
        Ok(())
    }

    fn inspect(&self, kind: WorldKind, _backend_id: &str) -> Result<WorldInspection, WorkerError> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        if kind == WorldKind::Host && self.host_setup_error {
            return Err(WorkerError::new(
                "host cloud-init failed: cloud-init final stage failed with exit status 1",
            ));
        }
        if self.missing {
            return Ok(WorldInspection::Missing);
        }
        if self.stopped || self.is_stopped.load(Ordering::SeqCst) {
            return Ok(WorldInspection::Stopped {
                reason: Some("crashed".into()),
            });
        }
        let mut inspected = world(kind, self.complete);
        if self.changed_guest_identity {
            inspected.access = GuestAccess::from_guest_ip(
                "192.0.2.2",
                vec!["ssh-ed25519 AAAACHANGED guest".into()],
            );
        }
        if self.changed_app_identity {
            let WorldApplication::Devcontainer { app_ssh } = &mut inspected.application else {
                panic!("changed app identity requires a devcontainer")
            };
            app_ssh.as_mut().unwrap().host_keys = vec!["ssh-ed25519 AAAACHANGED app".into()];
        }
        Ok(WorldInspection::Running(inspected))
    }

    fn start(&self, kind: WorldKind, _backend_id: &str) -> Result<World, WorkerError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.is_stopped.store(false, Ordering::SeqCst);
        Ok(world(kind, self.complete))
    }

    fn stop(&self, _kind: WorldKind, _backend_id: &str) -> Result<(), WorkerError> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        if self.stop_error {
            Err(WorkerError::new("shutdown timed out"))
        } else {
            self.is_stopped.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn disk_usage(&self, _kind: WorldKind, _disk_id: Uuid) -> Result<u64, WorkerError> {
        Ok(self.disk_usage_bytes)
    }
}

fn world(kind: WorldKind, complete: bool) -> World {
    World {
        access: GuestAccess::from_guest_ip("192.0.2.2", vec!["ssh-ed25519 AAAATEST guest".into()]),
        application: match kind {
            WorldKind::Devcontainer => WorldApplication::Devcontainer {
                app_ssh: complete.then(|| wt_control_protocol::AppSshAccess {
                    user: "vscode".into(),
                    port: 2222,
                    host_keys: vec!["ssh-ed25519 AAAAAPP app".into()],
                }),
            },
            WorldKind::Host => WorldApplication::Host {
                setup_complete: complete,
            },
            WorldKind::GithubCi => panic!("github-ci is not a retained world"),
        },
    }
}

pub(crate) fn create(name: &str) -> CreateInstance {
    CreateInstance {
        name: InstanceName::parse(name).unwrap(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        ssh_authorized_keys: vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example".into()],
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
        application: CreateApplication::Devcontainer {
            source: "git@example.test:repo.git".into(),
            git_base: "main".into(),
        },
    }
}

pub(crate) fn create_host(name: &str, user_data: &str) -> CreateInstance {
    CreateInstance {
        name: InstanceName::parse(name).unwrap(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        ssh_authorized_keys: vec!["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo wt@example".into()],
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
        application: CreateApplication::Host {
            user_data: user_data.into(),
        },
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
