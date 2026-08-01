use super::*;

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_fork_rejects_a_source_that_is_still_in_setup() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let source = unique_name("setup-source");
    let destination = InstanceName::parse(format!("{source}-fork")).unwrap();
    let instance = timings.run("create setup-only KVM world", || harness.create(&source));
    assert_eq!(instance.status, InstanceStatus::Setup);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 1
    );

    let error = call_api_result(
        harness.temp.path(),
        &harness.server_config_path,
        Operation::Fork(ForkInstance {
            source: source.clone(),
            name: destination.clone(),
        }),
    )
    .unwrap_err();
    assert!(
        error.contains("source world must be running") && error.contains("setup"),
        "unexpected setup-source fork error: {error}"
    );
    let instances = harness.sync_inventory();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].name, source);
    assert!(instances
        .iter()
        .all(|instance| instance.name != destination));
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 1
    );
    harness.delete(&source);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes
    );
}

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_fork_rejects_a_stopped_source_without_changing_its_disk_graph() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let source = unique_name("stopped-source");
    let destination = InstanceName::parse(format!("{source}-fork")).unwrap();
    let created = timings.run("create stopped-source KVM world", || {
        harness.create(&source)
    });
    assert_eq!(created.status, InstanceStatus::Setup);
    let agent = SshAgent::start(harness.temp.path(), &harness.git.git_key);
    let running = harness.finish_setup(&source, &agent);
    assert_eq!(running.status, InstanceStatus::Running);
    let backend_id = format!("wt-{}", running.id.simple());
    let output = cmd!(
        "virsh",
        "-c",
        wt_libvirt::LIBVIRT_URI,
        "destroy",
        &backend_id,
    )
    .output()
    .unwrap();
    ensure_success("stop KVM fork source", &output).unwrap();

    let error = call_api_result(
        harness.temp.path(),
        &harness.server_config_path,
        Operation::Fork(ForkInstance {
            source: source.clone(),
            name: destination.clone(),
        }),
    )
    .unwrap_err();
    assert!(
        error.contains("stopped"),
        "unexpected stopped-source fork error: {error}"
    );
    let Response::Instance { instance } = call_api(
        harness.temp.path(),
        &harness.server_config_path,
        Operation::Get {
            name: source.clone(),
        },
    ) else {
        panic!("expected stopped source response");
    };
    assert_eq!(instance.status, InstanceStatus::Error);
    assert!(instance
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("stopped")));
    assert!(harness
        .sync_inventory()
        .iter()
        .all(|instance| instance.name != destination));
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 1
    );
    harness.delete(&source);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes
    );
}
