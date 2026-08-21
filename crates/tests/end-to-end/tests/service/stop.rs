use super::*;

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
    let Response::Instances { instances, .. } = service(&temp, stopped_worker)
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
    assert_eq!(instance.status, InstanceStatus::Running);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn stopped_world_counts_only_used_disk_and_reacquires_capacity_on_start() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        disk_usage_bytes: 1536 * 1024 * 1024,
        ..Worker::default()
    };
    let service = Service::with_capacity_limit(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker.clone(),
        Gateway,
        Operations::default(),
        wt_workload_registry::Resources {
            vcpus: 2,
            memory_mib: 2048,
            disk_gib: 10,
        },
    );
    service
        .execute("tester", Operation::Create(create("first")))
        .unwrap();

    let Response::Instance { instance } = service
        .execute(
            "tester",
            Operation::Stop {
                name: InstanceName::parse("first").unwrap(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Stopped);
    assert_eq!(
        instance.last_error.as_deref(),
        Some("guest stopped (requested)")
    );
    assert_eq!(worker.stops.load(Ordering::SeqCst), 1);

    service
        .execute("tester", Operation::Create(create("second")))
        .unwrap();
    let error = service
        .execute(
            "tester",
            Operation::Start {
                name: InstanceName::parse("first").unwrap(),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Capacity);
    assert_eq!(error.capacity.unwrap().resource, CapacityResource::Disk);

    let Response::Instances {
        instances,
        disk_usage_bytes,
        ..
    } = service.execute("tester", Operation::List).unwrap()
    else {
        panic!()
    };
    let first = instances
        .iter()
        .find(|world| world.name.as_str() == "first")
        .unwrap();
    assert_eq!(disk_usage_bytes[&first.id], 1536 * 1024 * 1024);
}

#[test]
fn failed_stop_keeps_the_world_running_and_resources_reserved() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        stop_error: true,
        ..Worker::default()
    };
    let service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
        Gateway,
        Operations::default(),
        1024,
    );
    service
        .execute("tester", Operation::Create(create("first")))
        .unwrap();
    let error = service
        .execute(
            "tester",
            Operation::Stop {
                name: InstanceName::parse("first").unwrap(),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Backend);

    let error = service
        .execute("tester", Operation::Create(create("second")))
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::Capacity);
}
