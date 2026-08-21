use std::sync::atomic::Ordering;
use tempfile::TempDir;
use wt_control_protocol::{CapacityResource, InstanceName, InstanceStatus, Operation, Response};
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_workload_registry::Store;

#[path = "service/agent_tool_reports.rs"]
mod agent_tool_reports;
#[path = "service/stop.rs"]
mod stop;
#[path = "service/support.rs"]
mod support;
use support::{create, service, Gateway, UnavailableGateway, Worker};

#[test]
fn create_returns_a_running_host_and_reuses_an_identical_request() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let service = service(&temp, worker.clone());

    let Response::Instance { instance } = service
        .execute("tester", Operation::Create(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert_eq!(instance.vcpus, 1);
    assert_eq!(instance.memory_mib, 1024);
    assert_eq!(instance.disk_gib, 8);
    assert!(instance.ssh.is_some());
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 1);
    assert_eq!(worker.host_git_grants.lock().unwrap().len(), 1);

    let Response::Instance { instance: retry } = service
        .execute("tester", Operation::Create(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(retry.id, instance.id);
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 1);

    let mut changed = create("sample");
    changed.memory_mib += 1;
    let error = service
        .execute("tester", Operation::Create(changed))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Conflict);
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
fn worlds_share_capacity() {
    let temp = TempDir::new().unwrap();
    let service = Service::with_capacity_limit(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        Worker::default(),
        Gateway,
        Operations::default(),
        wt_workload_registry::Resources {
            vcpus: 2,
            memory_mib: 2048,
            disk_gib: 16,
        },
    );
    service
        .execute("tester", Operation::Create(create("first")))
        .unwrap();
    service
        .execute("tester", Operation::Create(create("second")))
        .unwrap();
    let error = service
        .execute("tester", Operation::Create(create("third")))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Capacity);
    assert_eq!(error.capacity.unwrap().resource, CapacityResource::Memory);
}

#[test]
fn failed_create_is_retained_until_delete() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        provision_error: true,
        ..Worker::default()
    };
    let service = service(&temp, worker.clone());
    let name = InstanceName::parse("sample").unwrap();

    let error = service
        .execute("tester", Operation::Create(create(name.as_str())))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);
    assert_eq!(
        Store::open(&temp.path().join("instances.db"))
            .unwrap()
            .get("tester", &name)
            .unwrap()
            .instance
            .status,
        InstanceStatus::Error
    );

    service
        .execute("tester", Operation::Delete { name: name.clone() })
        .unwrap();
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Store::open(&temp.path().join("instances.db"))
            .unwrap()
            .get("tester", &name),
        Err(wt_workload_registry::StoreError::NotFound)
    ));
}

#[test]
fn delete_keeps_registry_until_gateway_revocation_succeeds() {
    let temp = TempDir::new().unwrap();
    let store = Store::open(&temp.path().join("instances.db")).unwrap();
    let worker = Worker::default();
    Service::new(
        store,
        worker.clone(),
        Gateway,
        Operations::default(),
        u64::MAX,
    )
    .execute("tester", Operation::Create(create("sample")))
    .unwrap();
    let gateway = UnavailableGateway::default();
    let service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
        gateway.clone(),
        Operations::default(),
        u64::MAX,
    );

    let error = service
        .execute(
            "tester",
            Operation::Delete {
                name: InstanceName::parse("sample").unwrap(),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);
    assert_eq!(gateway.revocations.load(Ordering::SeqCst), 1);
    assert!(Store::open(&temp.path().join("instances.db"))
        .unwrap()
        .get("tester", &InstanceName::parse("sample").unwrap())
        .is_ok());
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
        let Response::Instances { instances, .. } = service(&temp, worker)
            .execute("tester", Operation::List)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(instances[0].status, InstanceStatus::Error);
    }
}
