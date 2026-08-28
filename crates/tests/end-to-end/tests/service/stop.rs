use super::*;

#[test]
fn stopped_world_can_be_started() {
    let temp = TempDir::new().unwrap();
    service(&temp, Worker::default())
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap();
    let stopped_worker = Worker {
        stopped: true,
        ..Worker::default()
    };
    let gateway = Gateway::default();
    let deactivated_worlds = gateway.deactivated_worlds.clone();
    let reconcile_service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        stopped_worker,
        gateway,
        Operations::default(),
        64 * 1024,
    );
    let Response::Worlds { worlds, .. } = reconcile_service
        .execute("tester", Operation::ListWorlds)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(worlds[0].status, WorldStatus::Stopped);
    assert_eq!(
        worlds[0].last_error.as_deref(),
        Some("guest stopped (crashed)")
    );
    assert_eq!(*deactivated_worlds.lock().unwrap(), [worlds[0].world_id]);

    let worker = Worker {
        stopped: true,
        ..Worker::default()
    };
    let starts = worker.starts.clone();
    let Response::World { world } = service(&temp, worker)
        .execute(
            "tester",
            Operation::StartWorld {
                world_id: worlds[0].world_id,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(world.status, WorldStatus::Running);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn list_does_not_reconcile_a_world_with_an_active_lifecycle_operation() {
    let temp = TempDir::new().unwrap();
    let Response::World { world } = service(&temp, Worker::default())
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    let operations = Operations::default();
    let worker = Worker {
        stopped: true,
        ..Worker::default()
    };
    let inspections = worker.inspections.clone();
    let service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker,
        Gateway::default(),
        operations.clone(),
        64 * 1024,
    );
    let operation = operations.try_lock_world(world.world_id).unwrap();

    let Response::Worlds { worlds, .. } = service.execute("tester", Operation::ListWorlds).unwrap()
    else {
        panic!()
    };
    assert_eq!(worlds[0].status, WorldStatus::Running);
    assert_eq!(inspections.load(Ordering::SeqCst), 0);

    drop(operation);
    let Response::Worlds { worlds, .. } = service.execute("tester", Operation::ListWorlds).unwrap()
    else {
        panic!()
    };
    assert_eq!(worlds[0].status, WorldStatus::Stopped);
}

#[test]
fn stopped_world_counts_only_used_disk_and_reacquires_capacity_on_start() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        disk_usage_bytes: 1536 * 1024 * 1024,
        ..Worker::default()
    };
    let gateway = Gateway::default();
    let deactivated_worlds = gateway.deactivated_worlds.clone();
    let service = Service::with_capacity_limit(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker.clone(),
        gateway,
        Operations::default(),
        wt_workload_registry::Resources {
            vcpus: 2,
            memory_mib: 2048,
            disk_gib: 10,
        },
    );
    let Response::World { world: first } = service
        .execute("tester", Operation::CreateWorld(create("first")))
        .unwrap()
    else {
        panic!()
    };

    let Response::World { world } = service
        .execute(
            "tester",
            Operation::StopWorld {
                world_id: first.world_id,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(world.status, WorldStatus::Stopped);
    assert_eq!(
        world.last_error.as_deref(),
        Some("guest stopped (requested)")
    );
    assert_eq!(worker.stops.load(Ordering::SeqCst), 1);
    assert_eq!(*deactivated_worlds.lock().unwrap(), [first.world_id]);

    service
        .execute("tester", Operation::CreateWorld(create("second")))
        .unwrap();
    let error = service
        .execute(
            "tester",
            Operation::StartWorld {
                world_id: world.world_id,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Capacity);
    assert_eq!(error.capacity.unwrap().resource, CapacityResource::Disk);

    let Response::Worlds {
        worlds,
        disk_usage_bytes,
        ..
    } = service.execute("tester", Operation::ListWorlds).unwrap()
    else {
        panic!()
    };
    let first = worlds
        .iter()
        .find(|world| world.name.as_str() == "first")
        .unwrap();
    assert_eq!(disk_usage_bytes[&first.world_id], 1536 * 1024 * 1024);
}

#[test]
fn failed_stop_keeps_the_world_running_and_resources_reserved() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        stop_error: true,
        ..Worker::default()
    };
    let service = Service::new(
        Store::open(&temp.path().join("worlds.db")).unwrap(),
        worker,
        Gateway::default(),
        Operations::default(),
        1024,
    );
    let Response::World { world: first } = service
        .execute("tester", Operation::CreateWorld(create("first")))
        .unwrap()
    else {
        panic!()
    };
    let error = service
        .execute(
            "tester",
            Operation::StopWorld {
                world_id: first.world_id,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);

    let error = service
        .execute("tester", Operation::CreateWorld(create("second")))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Capacity);
}
