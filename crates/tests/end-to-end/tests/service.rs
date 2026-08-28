use std::sync::{atomic::Ordering, Arc, Mutex};
use tempfile::TempDir;
use wt_control_protocol::{CapacityResource, Operation, Response, WorldName, WorldStatus};
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

    let Response::World { world } = service
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(world.status, WorldStatus::Running);
    assert_eq!(world.vcpus, 1);
    assert_eq!(world.memory_mib, 1024);
    assert_eq!(world.disk_gib, 8);
    assert!(world.ssh.is_some());
    assert_eq!(
        worker.provisioned_disks.lock().unwrap().as_slice(),
        &[world.world_id]
    );
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 1);
    assert_eq!(worker.host_git_grants.lock().unwrap().len(), 1);

    let Response::World { world: retry } = service
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(retry.world_id, world.world_id);
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 1);

    let mut changed = create("sample");
    changed.memory_mib += 1;
    let error = service
        .execute("tester", Operation::CreateWorld(changed))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Conflict);
}

#[test]
fn rename_keeps_the_world_uuid_and_disk_across_a_restart() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let service = service(&temp, worker.clone());
    let Response::World { world: created } = service
        .execute("tester", Operation::CreateWorld(create("before")))
        .unwrap()
    else {
        panic!()
    };

    let Response::World { world: renamed } = service
        .execute(
            "tester",
            Operation::RenameWorld {
                world_id: created.world_id,
                new_name: WorldName::parse("after").unwrap(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(renamed.world_id, created.world_id);
    assert_eq!(renamed.name.as_str(), "after");
    assert!(Store::open(&temp.path().join("worlds.db"))
        .unwrap()
        .get_owned_by_name("tester", &WorldName::parse("before").unwrap())
        .is_err());

    service
        .execute(
            "tester",
            Operation::StopWorld {
                world_id: renamed.world_id,
            },
        )
        .unwrap();
    let Response::World { world: restarted } = service
        .execute(
            "tester",
            Operation::StartWorld {
                world_id: renamed.world_id,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(restarted.world_id, created.world_id);

    service
        .execute(
            "tester",
            Operation::DeleteWorld {
                world_id: created.world_id,
            },
        )
        .unwrap();
    assert_eq!(
        worker.destroyed_disks.lock().unwrap().as_slice(),
        &[created.world_id]
    );
}

#[test]
fn world_names_are_unique_across_owners() {
    let temp = TempDir::new().unwrap();
    let service = service(&temp, Worker::default());
    service
        .execute("first", Operation::CreateWorld(create("shared")))
        .unwrap();

    let error = service
        .execute("second", Operation::CreateWorld(create("shared")))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Conflict);
}

#[test]
fn get_reconciles_only_the_requested_world() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    service(&temp, worker.clone())
        .execute("tester", Operation::CreateWorld(create("first")))
        .unwrap();
    service(&temp, worker.clone())
        .execute("tester", Operation::CreateWorld(create("second")))
        .unwrap();

    service(&temp, worker.clone())
        .execute(
            "tester",
            Operation::GetWorld {
                name: WorldName::parse("first").unwrap(),
            },
        )
        .unwrap();

    assert_eq!(worker.inspections.load(Ordering::SeqCst), 1);
}

#[test]
fn worlds_share_capacity() {
    let temp = TempDir::new().unwrap();
    let service = Service::with_capacity_limit(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        Worker::default(),
        Gateway::default(),
        Operations::default(),
        wt_workload_registry::Resources {
            vcpus: 2,
            memory_mib: 2048,
            disk_gib: 16,
        },
    );
    service
        .execute("tester", Operation::CreateWorld(create("first")))
        .unwrap();
    service
        .execute("tester", Operation::CreateWorld(create("second")))
        .unwrap();
    let error = service
        .execute("tester", Operation::CreateWorld(create("third")))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Capacity);
    assert_eq!(error.capacity.unwrap().resource, CapacityResource::Memory);
}

#[test]
fn failed_create_is_preserved_until_delete() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        provision_error: true,
        ..Worker::default()
    };
    let service = service(&temp, worker.clone());
    let name = WorldName::parse("sample").unwrap();

    let error = service
        .execute("tester", Operation::CreateWorld(create(name.as_str())))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);
    assert_eq!(
        Store::open(&temp.path().join("worlds.db"))
            .unwrap()
            .get_owned_by_name("tester", &name)
            .unwrap()
            .world
            .status,
        WorldStatus::Error
    );

    let world_id = Store::open(&temp.path().join("worlds.db"))
        .unwrap()
        .get_owned_by_name("tester", &name)
        .unwrap()
        .world
        .world_id;
    service
        .execute("tester", Operation::DeleteWorld { world_id })
        .unwrap();
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Store::open(&temp.path().join("worlds.db"))
            .unwrap()
            .get_owned_by_name("tester", &name),
        Err(wt_workload_registry::StoreError::NotFound)
    ));
}

#[test]
fn delete_deactivates_and_revokes_before_destroying_the_world() {
    let temp = TempDir::new().unwrap();
    let create_service = service(&temp, Worker::default());
    let Response::World { world } = create_service
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    let lifecycle_events = Arc::<Mutex<Vec<&'static str>>>::default();
    let worker = Worker {
        lifecycle_events: lifecycle_events.clone(),
        ..Worker::default()
    };
    let gateway = Gateway {
        lifecycle_events: lifecycle_events.clone(),
        ..Gateway::default()
    };
    let service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker.clone(),
        gateway.clone(),
        Operations::default(),
        u64::MAX,
    );

    service
        .execute(
            "tester",
            Operation::DeleteWorld {
                world_id: world.world_id,
            },
        )
        .unwrap();

    assert_eq!(
        lifecycle_events.lock().unwrap().as_slice(),
        ["deactivate", "revoke", "destroy"]
    );
    assert_eq!(
        gateway
            .deactivated_pane_observations
            .lock()
            .unwrap()
            .as_slice(),
        [world.world_id]
    );
    assert_eq!(gateway.revocations.load(Ordering::SeqCst), 1);
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_grant_revocation_preserves_destroying_intent() {
    let temp = TempDir::new().unwrap();
    let create_service = service(&temp, Worker::default());
    let Response::World { world } = create_service
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    let worker = Worker::default();
    let gateway = UnavailableGateway::default();
    let service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker.clone(),
        gateway.clone(),
        Operations::default(),
        u64::MAX,
    );

    let error = service
        .execute(
            "tester",
            Operation::DeleteWorld {
                world_id: world.world_id,
            },
        )
        .unwrap_err();

    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);
    assert_eq!(
        gateway
            .deactivated_pane_observations
            .lock()
            .unwrap()
            .as_slice(),
        [world.world_id]
    );
    assert_eq!(gateway.revocations.load(Ordering::SeqCst), 1);
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 0);
    let retained = Store::open(&temp.path().join("worlds.db"))
        .unwrap()
        .get_owned_by_name("tester", &WorldName::parse("sample").unwrap())
        .unwrap();
    assert_eq!(retained.world.status, WorldStatus::Destroying);
}

#[test]
fn failed_world_destruction_happens_after_grant_revocation() {
    let temp = TempDir::new().unwrap();
    let create_service = service(&temp, Worker::default());
    let Response::World { world } = create_service
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    let worker = Worker {
        destroy_error: true,
        ..Worker::default()
    };
    let gateway = Gateway::default();
    let service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker.clone(),
        gateway.clone(),
        Operations::default(),
        u64::MAX,
    );

    let error = service
        .execute(
            "tester",
            Operation::DeleteWorld {
                world_id: world.world_id,
            },
        )
        .unwrap_err();

    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.revocations.load(Ordering::SeqCst), 1);
    assert_eq!(
        gateway
            .deactivated_pane_observations
            .lock()
            .unwrap()
            .as_slice(),
        [world.world_id]
    );
    assert_eq!(
        Store::open(&temp.path().join("worlds.db"))
            .unwrap()
            .get_owned_by_id("tester", world.world_id)
            .unwrap()
            .world
            .status,
        WorldStatus::Error
    );
}

#[test]
fn delete_rejects_an_unknown_world_id_without_side_effects() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let create_service = service(&temp, worker.clone());
    let name = WorldName::parse("sample").unwrap();
    let Response::World { world } = create_service
        .execute("tester", Operation::CreateWorld(create(name.as_str())))
        .unwrap()
    else {
        panic!()
    };
    let gateway = UnavailableGateway::default();
    let service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker.clone(),
        gateway.clone(),
        Operations::default(),
        u64::MAX,
    );

    let error = service
        .execute(
            "tester",
            Operation::DeleteWorld {
                world_id: wt_control_protocol::WorldId::new(),
            },
        )
        .unwrap_err();

    assert_eq!(error.code, wt_control_protocol::ErrorCode::NotFound);
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 0);
    assert_eq!(gateway.revocations.load(Ordering::SeqCst), 0);
    assert_eq!(
        Store::open(&temp.path().join("worlds.db"))
            .unwrap()
            .get_owned_by_name("tester", &name)
            .unwrap()
            .world
            .world_id,
        world.world_id
    );
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
            .execute("tester", Operation::CreateWorld(create("sample")))
            .unwrap();
        let Response::Worlds { worlds, .. } = service(&temp, worker)
            .execute("tester", Operation::ListWorlds)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(worlds[0].status, WorldStatus::Error);
    }
}

#[test]
fn reconciliation_recovers_an_errored_world_that_is_healthy_again() {
    let temp = TempDir::new().unwrap();
    service(&temp, Worker::default())
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap();

    let Response::Worlds { worlds, .. } = service(
        &temp,
        Worker {
            changed_guest_identity: true,
            ..Worker::default()
        },
    )
    .execute("tester", Operation::ListWorlds)
    .unwrap() else {
        panic!()
    };
    assert_eq!(worlds[0].status, WorldStatus::Error);

    let Response::Worlds { worlds, .. } = service(&temp, Worker::default())
        .execute("tester", Operation::ListWorlds)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(worlds[0].status, WorldStatus::Running);
    assert_eq!(worlds[0].last_error, None);
}
