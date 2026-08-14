use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Condvar, Mutex,
};
use tempfile::TempDir;
use uuid::Uuid;
use wt_api::{
    Capacity, CapacityResource, CreateInstance, ForkInstance, Instance, InstanceName,
    InstanceStatus, Operation, Response, SshAccess,
};
use wt_provider::{
    ForkError, ForkSpec, ProvisionSpec, WorkerError, World, WorldInspection, WorldWorker,
};
use wt_server::operations::Operations;
use wt_server::service::{AgentGitGateway, Service};
use wt_server::store::{Store, StoredInstance};

#[derive(Clone, Default)]
struct Worker {
    provisions: Arc<AtomicUsize>,
    destroys: Arc<AtomicUsize>,
    inspections: Arc<AtomicUsize>,
    starts: Arc<AtomicUsize>,
    destroyed_disks: Arc<Mutex<Vec<Vec<Uuid>>>>,
    complete: bool,
    provision_gate: Option<Arc<(Mutex<bool>, Condvar)>>,
    missing: bool,
    changed_guest_identity: bool,
    changed_app_identity: bool,
    provision_error: bool,
    stopped: bool,
}

#[derive(Clone, Default)]
struct Gateway;

#[derive(Clone, Default)]
struct UnavailableGateway {
    revocations: Arc<AtomicUsize>,
}

impl AgentGitGateway for Gateway {
    fn reserve(
        &self,
        world_id: Uuid,
        _source: &str,
        _base: &str,
    ) -> Result<wt_agent_git::Grant, String> {
        Ok(wt_agent_git::Grant {
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
    ) -> Result<wt_agent_git::Grant, String> {
        Ok(wt_agent_git::Grant {
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

#[test]
fn get_reconciles_only_the_requested_world() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    service(&temp, worker.clone())
        .execute("tester", Operation::Create(create("first")))
        .unwrap();
    service(&temp, worker.clone())
        .execute("tester", Operation::Create(create("second")))
        .unwrap();

    service(&temp, worker.clone())
        .execute(
            "tester",
            Operation::Get {
                name: InstanceName::parse("first").unwrap(),
            },
        )
        .unwrap();

    assert_eq!(worker.inspections.load(Ordering::SeqCst), 1);
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

fn create(name: &str) -> CreateInstance {
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

fn service(temp: &TempDir, worker: Worker) -> Service<Worker, Gateway> {
    Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
        Gateway,
        Operations::default(),
        64 * 1024,
    )
}

#[test]
fn create_returns_setup_ready_world_synchronously() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let calls = worker.provisions.clone();
    let Response::Instance { instance } = service(&temp, worker)
        .execute("tester", Operation::Create(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Setup);
    assert_eq!(instance.vcpus, 1);
    assert_eq!(instance.memory_mib, 1024);
    assert_eq!(instance.disk_gib, 8);
    assert_eq!(instance.git_prefix, "wt/");
    assert!(instance.ssh.is_some());
    assert!(instance.app_ssh.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn list_reconciles_completed_setup_to_running() {
    let temp = TempDir::new().unwrap();
    service(&temp, Worker::default())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap();
    let Response::Instances { instances } = service(
        &temp,
        Worker {
            complete: true,
            ..Worker::default()
        },
    )
    .execute("tester", Operation::List)
    .unwrap() else {
        panic!()
    };
    assert_eq!(instances[0].status, InstanceStatus::Running);
    assert_eq!(instances[0].vcpus, 1);
    assert_eq!(instances[0].memory_mib, 1024);
    assert_eq!(instances[0].disk_gib, 8);
    assert!(instances[0].app_ssh.is_some());
}

#[test]
fn stopped_world_can_be_started() {
    let temp = TempDir::new().unwrap();
    service(&temp, Worker::default())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap();
    let stopped_worker = Worker {
        stopped: true,
        ..Worker::default()
    };
    let Response::Instances { instances } = service(&temp, stopped_worker)
        .execute("tester", Operation::List)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instances[0].status, InstanceStatus::Stopped);
    assert_eq!(
        instances[0].last_error.as_deref(),
        Some("guest stopped (crashed)")
    );

    let worker = Worker {
        stopped: true,
        ..Worker::default()
    };
    let starts = worker.starts.clone();
    let Response::Instance { instance } = service(&temp, worker)
        .execute(
            "tester",
            Operation::Start {
                name: InstanceName::parse("sample").unwrap(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Setup);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn create_rejects_full_memory_capacity_without_provisioning() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker.clone(),
        Gateway,
        Operations::default(),
        1024,
    );
    service
        .execute("tester", Operation::Create(create("first")))
        .unwrap();
    let error = service
        .execute("tester", Operation::Create(create("second")))
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Capacity);
    assert_eq!(
        error.capacity,
        Some(Capacity {
            resource: CapacityResource::Memory,
            total: 1024,
            reserved: 1024,
            requested: 1024,
        })
    );
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 1);
}

#[test]
fn worlds_share_cpu_and_disk_capacity() {
    for (resource, limit) in [
        (
            CapacityResource::Cpu,
            wt_registry::Resources {
                vcpus: 1,
                memory_mib: 64 * 1024,
                disk_gib: 1024,
            },
        ),
        (
            CapacityResource::Disk,
            wt_registry::Resources {
                vcpus: 64,
                memory_mib: 64 * 1024,
                disk_gib: 8,
            },
        ),
    ] {
        let temp = TempDir::new().unwrap();
        let service = Service::with_capacity_limit(
            Store::open(&temp.path().join("instances.db")).unwrap(),
            Worker::default(),
            Gateway,
            Operations::default(),
            limit,
        );
        service
            .execute("tester", Operation::Create(create("first")))
            .unwrap();
        let error = service
            .execute("tester", Operation::Create(create("second")))
            .unwrap_err();
        assert_eq!(error.code, wt_api::ErrorCode::Capacity);
        assert_eq!(error.capacity.unwrap().resource, resource);
    }
}

#[test]
fn create_rejects_a_world_larger_than_host_memory() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let mut request = create("sample");
    request.memory_mib = 2048;
    let error = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker.clone(),
        Gateway,
        Operations::default(),
        1024,
    )
    .execute("tester", Operation::Create(request))
    .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::InvalidRequest);
    assert_eq!(error.capacity, None);
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 0);
}

#[test]
fn concurrent_creates_cannot_claim_the_same_memory() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_owned();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let mut creates = Vec::new();
    for name in ["first", "second"] {
        let root = root.clone();
        let barrier = barrier.clone();
        creates.push(std::thread::spawn(move || {
            let service = Service::new(
                Store::open(&root.join("instances.db")).unwrap(),
                Worker::default(),
                Gateway,
                Operations::default(),
                1024,
            );
            barrier.wait();
            service.execute("tester", Operation::Create(create(name)))
        }));
    }
    barrier.wait();
    let results = creates
        .into_iter()
        .map(|create| create.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(
                |result| matches!(result, Err(error) if error.code == wt_api::ErrorCode::Capacity)
            )
            .count(),
        1
    );
}

#[test]
fn delete_removes_setup_world() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let destroys = worker.destroys.clone();
    service(&temp, worker.clone())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap();
    service(&temp, worker)
        .execute(
            "tester",
            Operation::Delete {
                name: InstanceName::parse("sample").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(destroys.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_create_keeps_registry_until_grant_revocation_succeeds() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        provision_error: true,
        ..Worker::default()
    };
    let gateway = UnavailableGateway::default();
    let revocations = gateway.revocations.clone();
    let error = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker.clone(),
        gateway,
        Operations::default(),
        64 * 1024,
    )
    .execute("tester", Operation::Create(create("sample")))
    .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Backend);
    assert_eq!(error.message, "provision failed");
    assert_eq!(revocations.load(Ordering::SeqCst), 1);
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 0);

    let stored = Store::open(&temp.path().join("instances.db"))
        .unwrap()
        .get("tester", &InstanceName::parse("sample").unwrap())
        .unwrap();
    assert_eq!(stored.instance.status, InstanceStatus::Error);
    assert_eq!(
        stored.instance.last_error.as_deref(),
        Some("provision failed; Git grant revocation failed: gateway unavailable")
    );

    service(&temp, worker.clone())
        .execute(
            "tester",
            Operation::Delete {
                name: InstanceName::parse("sample").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Store::open(&temp.path().join("instances.db"))
            .unwrap()
            .get("tester", &InstanceName::parse("sample").unwrap()),
        Err(wt_server::store::StoreError::NotFound)
    ));
}

#[test]
fn fork_is_unavailable_for_every_world() {
    let temp = TempDir::new().unwrap();
    let error = service(&temp, Worker::default())
        .execute(
            "tester",
            Operation::Fork(ForkInstance {
                source: InstanceName::parse("source").unwrap(),
                name: InstanceName::parse("fork").unwrap(),
            }),
        )
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::InvalidRequest);
    assert_eq!(error.message, "worlds cannot be forked");
}

#[test]
fn repeated_create_resumes_only_identical_setup() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let provisions = worker.provisions.clone();
    let mut first = create("sample");
    first.git_base = "feature".into();
    let Response::Instance { instance: original } = service(&temp, worker.clone())
        .execute("tester", Operation::Create(first))
        .unwrap()
    else {
        panic!()
    };
    let mut same = create("sample");
    same.git_base = "feature".into();
    let Response::Instance { instance: resumed } = service(&temp, worker.clone())
        .execute("tester", Operation::Create(same))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(resumed.id, original.id);
    assert_eq!(provisions.load(Ordering::SeqCst), 1);

    let mut different = create("sample");
    different.git_base = "other".into();
    let error = service(&temp, worker)
        .execute("tester", Operation::Create(different))
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Conflict);
}

#[test]
fn create_retry_fingerprint_includes_resources_and_authorized_keys() {
    for change in ["resources", "keys"] {
        let temp = TempDir::new().unwrap();
        let worker = Worker::default();
        service(&temp, worker.clone())
            .execute("tester", Operation::Create(create("sample")))
            .unwrap();
        let mut different = create("sample");
        if change == "resources" {
            different.memory_mib += 1;
        } else {
            different.ssh_authorized_keys[0].push_str(" changed-comment");
        }
        let error = service(&temp, worker)
            .execute("tester", Operation::Create(different))
            .unwrap_err();
        assert_eq!(error.code, wt_api::ErrorCode::Conflict, "{change}");
    }
}

#[test]
fn matching_retry_waits_for_synchronous_preparation() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_owned();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let worker = Worker {
        provision_gate: Some(gate.clone()),
        ..Worker::default()
    };
    let operations = Operations::default();
    let creator = std::thread::spawn({
        let root = root.clone();
        let worker = worker.clone();
        let operations = operations.clone();
        move || {
            Service::new(
                Store::open(&root.join("instances.db")).unwrap(),
                worker,
                Gateway,
                operations,
                64 * 1024,
            )
            .execute("tester", Operation::Create(create("sample")))
            .unwrap()
        }
    });
    while worker.provisions.load(Ordering::SeqCst) == 0 {
        std::thread::yield_now();
    }
    let delete_error = Service::new(
        Store::open(&root.join("instances.db")).unwrap(),
        Worker::default(),
        Gateway,
        operations.clone(),
        64 * 1024,
    )
    .execute(
        "tester",
        Operation::Delete {
            name: InstanceName::parse("sample").unwrap(),
        },
    )
    .unwrap_err();
    assert_eq!(delete_error.code, wt_api::ErrorCode::Conflict);
    let (sent, received) = mpsc::channel();
    let retry = std::thread::spawn({
        let root = root.clone();
        let operations = operations.clone();
        move || {
            let response = Service::new(
                Store::open(&root.join("instances.db")).unwrap(),
                Worker::default(),
                Gateway,
                operations,
                64 * 1024,
            )
            .execute("tester", Operation::Create(create("sample")))
            .unwrap();
            sent.send(response).unwrap();
        }
    });
    assert!(received
        .recv_timeout(std::time::Duration::from_millis(50))
        .is_err());
    let (released, wake) = &*gate;
    *released.lock().unwrap() = true;
    wake.notify_all();
    let Response::Instance { instance } = received.recv().unwrap() else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Setup);
    creator.join().unwrap();
    retry.join().unwrap();
}

#[test]
fn reconciliation_marks_missing_or_changed_worlds_as_error() {
    for worker in [
        Worker {
            missing: true,
            ..Worker::default()
        },
        Worker {
            changed_guest_identity: true,
            ..Worker::default()
        },
    ] {
        let temp = TempDir::new().unwrap();
        service(&temp, Worker::default())
            .execute("tester", Operation::Create(create("sample")))
            .unwrap();
        let Response::Instances { instances } = service(&temp, worker)
            .execute("tester", Operation::List)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(instances[0].status, InstanceStatus::Error);
    }
}

#[test]
fn reconciliation_rejects_changed_app_identity() {
    let temp = TempDir::new().unwrap();
    service(&temp, Worker::default())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap();
    service(
        &temp,
        Worker {
            complete: true,
            ..Worker::default()
        },
    )
    .execute("tester", Operation::List)
    .unwrap();
    let Response::Instances { instances } = service(
        &temp,
        Worker {
            complete: true,
            changed_app_identity: true,
            ..Worker::default()
        },
    )
    .execute("tester", Operation::List)
    .unwrap() else {
        panic!()
    };
    assert_eq!(instances[0].status, InstanceStatus::Error);
}

#[test]
fn startup_recovery_marks_provisioning_as_error() {
    let temp = TempDir::new().unwrap();
    let store = Store::open(&temp.path().join("instances.db")).unwrap();
    let name = InstanceName::parse("sample").unwrap();
    let id = Uuid::new_v4();
    store
        .insert(&StoredInstance {
            instance: Instance {
                id,
                name: name.clone(),
                owner: "tester".into(),
                status: InstanceStatus::Provisioning,
                source: "git@example.test:repo.git".into(),
                git_base: "main".into(),
                git_prefix: "sample/".into(),
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
                guest_ip: None,
                last_error: None,
                ssh: None,
                app_ssh: None,
            },
            backend_id: format!("wt-{}", id.simple()),
            head_disk_id: Uuid::new_v4(),
            setup_fingerprint: "test".into(),
            gateway_grant_id: "grant-test".into(),
        })
        .unwrap();
    store.reconcile_interrupted().unwrap();
    assert_eq!(
        store.get("tester", &name).unwrap().instance.status,
        InstanceStatus::Error
    );
}
