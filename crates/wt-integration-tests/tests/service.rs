use std::sync::{atomic::Ordering, mpsc, Arc, Condvar, Mutex};
use tempfile::TempDir;
use uuid::Uuid;
use wt_api::{
    Capacity, CapacityResource, CreateApplication, ForkInstance, Instance, InstanceApplication,
    InstanceName, InstanceStatus, Operation, Response,
};
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::store::{Store, StoredApplication, StoredInstance};

#[path = "service/host_failure.rs"]
mod host_failure;
#[path = "service/support.rs"]
mod support;
use support::{
    create, create_host, service, Gateway, RejectingGateway, UnavailableGateway, Worker,
};

#[test]
fn host_create_returns_setup_then_reconciles_running() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let user_data = "#cloud-config\nruncmd:\n  - touch /host-ready\n";
    let service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker.clone(),
        RejectingGateway,
        Operations::default(),
        u64::MAX,
    );

    let Response::Instance { instance } = service
        .execute(
            "tester",
            Operation::Create(create_host("ubuntu", user_data)),
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Setup);
    assert_eq!(instance.kind(), wt_api::WorldKind::Host);
    assert!(instance.ssh.is_some());
    assert!(instance.application.app_ssh().is_none());
    assert_eq!(
        worker.host_user_data.lock().unwrap().as_slice(),
        &[user_data]
    );

    let Response::Instance { instance: retry } = service
        .execute(
            "tester",
            Operation::Create(create_host("ubuntu", user_data)),
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(retry.id, instance.id);
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 1);

    let Response::Instances { instances } = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        Worker {
            complete: true,
            ..Worker::default()
        },
        RejectingGateway,
        Operations::default(),
        u64::MAX,
    )
    .execute("tester", Operation::List)
    .unwrap() else {
        panic!()
    };
    assert_eq!(instances[0].status, InstanceStatus::Running);

    let error = service
        .execute(
            "tester",
            Operation::Create(create_host(
                "ubuntu",
                "#cloud-config\nruncmd:\n  - touch /different\n",
            )),
        )
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Conflict);

    service
        .execute(
            "tester",
            Operation::Delete {
                name: InstanceName::parse("ubuntu").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
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
    let InstanceApplication::Devcontainer {
        git_prefix,
        app_ssh,
        ..
    } = &instance.application
    else {
        panic!("expected devcontainer")
    };
    assert_eq!(git_prefix, "wt/");
    assert!(instance.ssh.is_some());
    assert!(app_ssh.is_none());
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
    assert!(instances[0].application.app_ssh().is_some());
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
    set_git_base(&mut first, "feature");
    let Response::Instance { instance: original } = service(&temp, worker.clone())
        .execute("tester", Operation::Create(first))
        .unwrap()
    else {
        panic!()
    };
    let mut same = create("sample");
    set_git_base(&mut same, "feature");
    let Response::Instance { instance: resumed } = service(&temp, worker.clone())
        .execute("tester", Operation::Create(same))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(resumed.id, original.id);
    assert_eq!(provisions.load(Ordering::SeqCst), 1);

    let mut different = create("sample");
    set_git_base(&mut different, "other");
    let error = service(&temp, worker)
        .execute("tester", Operation::Create(different))
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Conflict);
}

#[test]
fn repeated_create_does_not_reopen_a_running_devcontainer() {
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

    let error = service(&temp, Worker::default())
        .execute("tester", Operation::Create(create("sample")))
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
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
                guest_ip: None,
                last_error: None,
                ssh: None,
                application: InstanceApplication::Devcontainer {
                    source: "git@example.test:repo.git".into(),
                    git_base: "main".into(),
                    git_prefix: "sample/".into(),
                    app_ssh: None,
                },
            },
            backend_id: format!("wt-{}", id.simple()),
            head_disk_id: Uuid::new_v4(),
            setup_fingerprint: "test".into(),
            application: StoredApplication::Devcontainer {
                gateway_grant_id: "grant-test".into(),
            },
        })
        .unwrap();
    store.reconcile_interrupted().unwrap();
    assert_eq!(
        store.get("tester", &name).unwrap().instance.status,
        InstanceStatus::Error
    );
}

fn set_git_base(request: &mut wt_api::CreateInstance, value: &str) {
    let CreateApplication::Devcontainer { git_base, .. } = &mut request.application else {
        panic!("expected devcontainer request")
    };
    *git_base = value.to_owned();
}
