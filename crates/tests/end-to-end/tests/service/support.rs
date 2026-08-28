use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};
use tempfile::TempDir;
use wt_control_protocol::{CreateWorld, WorldId, WorldName};
use wt_guest::{GuestAccess, WorldInspection, WorldProvisionSpec, WorldWorker};
use wt_libvirt_kvm::WorkerError;
use wt_server::operations::Operations;
use wt_server::service::{AgentToolGateway, Service};
use wt_workload_registry::Store;

#[derive(Clone, Default)]
pub(crate) struct Worker {
    pub(crate) provisions: Arc<AtomicUsize>,
    pub(crate) destroys: Arc<AtomicUsize>,
    pub(crate) inspections: Arc<AtomicUsize>,
    pub(crate) starts: Arc<AtomicUsize>,
    pub(crate) stops: Arc<AtomicUsize>,
    pub(crate) destroyed_disks: Arc<Mutex<Vec<WorldId>>>,
    pub(crate) provisioned_disks: Arc<Mutex<Vec<WorldId>>>,
    pub(crate) host_git_grants: Arc<Mutex<Vec<String>>>,
    pub(crate) provision_gate: Option<Arc<(Mutex<bool>, Condvar)>>,
    pub(crate) missing: bool,
    pub(crate) changed_guest_identity: bool,
    pub(crate) provision_error: bool,
    pub(crate) destroy_error: bool,
    pub(crate) stopped: bool,
    pub(crate) is_stopped: Arc<AtomicBool>,
    pub(crate) stop_error: bool,
    pub(crate) disk_usage_bytes: u64,
    pub(crate) lifecycle_events: Arc<Mutex<Vec<&'static str>>>,
}

#[derive(Clone, Default)]
pub(crate) struct Gateway {
    pub(crate) deactivated_pane_observations: Arc<Mutex<Vec<WorldId>>>,
    pub(crate) revocations: Arc<AtomicUsize>,
    pub(crate) lifecycle_events: Arc<Mutex<Vec<&'static str>>>,
}

#[derive(Clone, Default)]
pub(crate) struct UnavailableGateway {
    pub(crate) revocations: Arc<AtomicUsize>,
    pub(crate) deactivated_pane_observations: Arc<Mutex<Vec<WorldId>>>,
}

impl AgentToolGateway for Gateway {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String> {
        Ok(wt_agent_tool_gateway::Grant {
            token: format!("token-{world_id}"),
        })
    }

    fn revoke(&self, _world_id: WorldId) -> Result<(), String> {
        self.lifecycle_events.lock().unwrap().push("revoke");
        self.revocations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn pane_observations(
        &self,
        _world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneObservationSnapshot>, String> {
        Ok(Vec::new())
    }

    fn activate_pane_observations(&self, _world_id: WorldId) -> Result<(), String> {
        Ok(())
    }

    fn deactivate_pane_observations(&self, world_id: WorldId) -> Result<(), String> {
        self.lifecycle_events.lock().unwrap().push("deactivate");
        self.deactivated_pane_observations
            .lock()
            .unwrap()
            .push(world_id);
        Ok(())
    }
}

impl AgentToolGateway for UnavailableGateway {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String> {
        Ok(wt_agent_tool_gateway::Grant {
            token: format!("token-{world_id}"),
        })
    }

    fn revoke(&self, _world_id: WorldId) -> Result<(), String> {
        self.revocations.fetch_add(1, Ordering::SeqCst);
        Err("gateway unavailable".to_owned())
    }

    fn pane_observations(
        &self,
        _world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneObservationSnapshot>, String> {
        Ok(Vec::new())
    }

    fn activate_pane_observations(&self, _world_id: WorldId) -> Result<(), String> {
        Ok(())
    }

    fn deactivate_pane_observations(&self, world_id: WorldId) -> Result<(), String> {
        self.deactivated_pane_observations
            .lock()
            .unwrap()
            .push(world_id);
        Ok(())
    }
}

impl WorldWorker for Worker {
    fn provision(
        &self,
        spec: WorldProvisionSpec<'_>,
        _log: &mut dyn std::io::Write,
    ) -> Result<GuestAccess, WorkerError> {
        self.provisions.fetch_add(1, Ordering::SeqCst);
        self.provisioned_disks.lock().unwrap().push(spec.world_id);
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
        self.host_git_grants
            .lock()
            .unwrap()
            .push(spec.git_grant.to_owned());
        Ok(world())
    }

    fn destroy(&self, world_id: WorldId) -> Result<(), WorkerError> {
        self.lifecycle_events.lock().unwrap().push("destroy");
        self.destroys.fetch_add(1, Ordering::SeqCst);
        self.destroyed_disks.lock().unwrap().push(world_id);
        if self.destroy_error {
            return Err(WorkerError::new("destroy failed"));
        }
        Ok(())
    }

    fn inspect(&self, _world_id: WorldId) -> Result<WorldInspection, WorkerError> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        if self.missing {
            return Ok(WorldInspection::Missing);
        }
        if self.stopped || self.is_stopped.load(Ordering::SeqCst) {
            return Ok(WorldInspection::Stopped {
                reason: Some("crashed".into()),
            });
        }
        let mut inspected = world();
        if self.changed_guest_identity {
            inspected = GuestAccess::from_guest_ip(
                "192.0.2.2",
                vec!["ssh-ed25519 AAAACHANGED guest".into()],
            );
        }
        Ok(WorldInspection::Running(inspected))
    }

    fn start(&self, _world_id: WorldId) -> Result<GuestAccess, WorkerError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.is_stopped.store(false, Ordering::SeqCst);
        Ok(world())
    }

    fn stop(&self, _world_id: WorldId) -> Result<(), WorkerError> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        if self.stop_error {
            Err(WorkerError::new("shutdown timed out"))
        } else {
            self.is_stopped.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn disk_usage(&self, _world_id: WorldId) -> Result<u64, WorkerError> {
        Ok(self.disk_usage_bytes)
    }
}

fn world() -> GuestAccess {
    GuestAccess::from_guest_ip("192.0.2.2", vec!["ssh-ed25519 AAAATEST guest".into()])
}

pub(crate) fn create(name: &str) -> CreateWorld {
    CreateWorld {
        name: WorldName::parse(name).unwrap(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
    }
}

pub(crate) fn service(temp: &TempDir, worker: Worker) -> Service<Worker, Gateway> {
    Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker,
        Gateway::default(),
        Operations::default(),
        64 * 1024,
    )
}
