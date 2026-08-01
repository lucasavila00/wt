use super::*;

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_nested_forks_survive_parent_and_source_deletion_and_collect_every_disk() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let source = unique_name("chain");
    let child = InstanceName::parse(format!("{source}-child")).unwrap();
    let grandchild = InstanceName::parse(format!("{source}-grandchild")).unwrap();
    let created = timings.run("create fork-chain source", || harness.create(&source));
    assert_eq!(created.status, InstanceStatus::Setup);
    let agent = SshAgent::start(harness.temp.path(), &harness.git.git_key);
    let source_instance = harness.finish_setup(&source, &agent);
    assert_eq!(source_instance.status, InstanceStatus::Running);
    harness.sync_inventory();
    run_guest(
        &harness,
        &source,
        "printf 'root\n' > /workspace/wt-chain-root",
        "write root fork-chain marker",
    );

    let child_instance = timings.run("fork first KVM generation", || {
        harness.fork(&source, &child)
    });
    assert_eq!(child_instance.status, InstanceStatus::Running);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 3
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &child,
        "set -eu; test \"$(cat /workspace/wt-chain-root)\" = root; printf 'child\n' > /workspace/wt-chain-child",
        "verify child inheritance and write child marker",
    );
    run_guest(
        &harness,
        &source,
        "test ! -e /workspace/wt-chain-child",
        "verify first-generation isolation",
    );

    let grandchild_instance = timings.run("fork second KVM generation", || {
        harness.fork(&child, &grandchild)
    });
    assert_eq!(grandchild_instance.status, InstanceStatus::Running);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 5
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &grandchild,
        "set -eu; test \"$(cat /workspace/wt-chain-root)\" = root; test \"$(cat /workspace/wt-chain-child)\" = child; printf 'grandchild\n' > /workspace/wt-chain-grandchild",
        "verify grandchild inheritance and write grandchild marker",
    );
    run_guest(
        &harness,
        &child,
        "test ! -e /workspace/wt-chain-grandchild",
        "verify second-generation isolation",
    );
    let machine_ids = [&source, &child, &grandchild].map(|name| {
        guest_output(
            &harness,
            name,
            "cat /etc/machine-id",
            "read chain machine ID",
        )
    });
    assert!(machine_ids.iter().all(|id| !id.trim().is_empty()));
    assert_ne!(machine_ids[0].trim(), machine_ids[1].trim());
    assert_ne!(machine_ids[0].trim(), machine_ids[2].trim());
    assert_ne!(machine_ids[1].trim(), machine_ids[2].trim());

    harness.delete(&child);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 4
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &grandchild,
        "set -eu; test \"$(cat /workspace/wt-chain-root)\" = root; test \"$(cat /workspace/wt-chain-child)\" = child; test \"$(cat /workspace/wt-chain-grandchild)\" = grandchild",
        "verify grandchild after deleting its direct parent",
    );
    run_guest(
        &harness,
        &source,
        "test \"$(cat /workspace/wt-chain-root)\" = root",
        "verify source after deleting fork-chain parent",
    );

    harness.delete(&source);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 3
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &grandchild,
        "set -eu; test \"$(cat /workspace/wt-chain-root)\" = root; test \"$(cat /workspace/wt-chain-child)\" = child; test \"$(cat /workspace/wt-chain-grandchild)\" = grandchild",
        "verify grandchild after deleting the original source",
    );

    harness.delete(&grandchild);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes
    );
    assert!(harness.sync_inventory().is_empty());
}

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_sibling_forks_capture_successive_source_points_and_survive_source_deletion() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let source = unique_name("siblings");
    let first = InstanceName::parse(format!("{source}-first")).unwrap();
    let second = InstanceName::parse(format!("{source}-second")).unwrap();
    let created = timings.run("create sibling-fork source", || harness.create(&source));
    assert_eq!(created.status, InstanceStatus::Setup);
    let agent = SshAgent::start(harness.temp.path(), &harness.git.git_key);
    assert_eq!(
        harness.finish_setup(&source, &agent).status,
        InstanceStatus::Running
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &source,
        "printf 'root\n' > /workspace/wt-sibling-root",
        "write sibling root marker",
    );

    assert_eq!(
        timings
            .run("fork first sibling", || harness.fork(&source, &first))
            .status,
        InstanceStatus::Running
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &source,
        "printf 'between\n' > /workspace/wt-sibling-between",
        "write between-siblings marker",
    );
    assert_eq!(
        timings
            .run("fork second sibling", || harness.fork(&source, &second))
            .status,
        InstanceStatus::Running
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &source,
        "printf 'after\n' > /workspace/wt-sibling-after",
        "write post-siblings source marker",
    );
    run_guest(
        &harness,
        &first,
        "set -eu; test \"$(cat /workspace/wt-sibling-root)\" = root; test ! -e /workspace/wt-sibling-between; test ! -e /workspace/wt-sibling-after",
        "verify first sibling captured the first source point",
    );
    run_guest(
        &harness,
        &second,
        "set -eu; test \"$(cat /workspace/wt-sibling-root)\" = root; test \"$(cat /workspace/wt-sibling-between)\" = between; test ! -e /workspace/wt-sibling-after",
        "verify second sibling captured the later source point",
    );
    run_guest(
        &harness,
        &source,
        "set -eu; test \"$(cat /workspace/wt-sibling-root)\" = root; test \"$(cat /workspace/wt-sibling-between)\" = between; test \"$(cat /workspace/wt-sibling-after)\" = after",
        "verify source kept all successive writes",
    );
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 5
    );

    harness.delete(&source);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 4
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &first,
        "test \"$(cat /workspace/wt-sibling-root)\" = root",
        "verify first sibling after source deletion",
    );
    run_guest(
        &harness,
        &second,
        "test \"$(cat /workspace/wt-sibling-between)\" = between",
        "verify second sibling after source deletion",
    );

    harness.delete(&second);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 2
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &first,
        "set -eu; test \"$(cat /workspace/wt-sibling-root)\" = root; test ! -e /workspace/wt-sibling-between",
        "verify first sibling after deleting later sibling",
    );
    harness.delete(&first);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes
    );
}
