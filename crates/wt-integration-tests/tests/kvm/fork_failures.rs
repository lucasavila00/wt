use super::*;

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_post_pivot_fork_failure_leaves_source_retryable() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let source = unique_name("retry");
    let destination = InstanceName::parse(format!("{source}-fork")).unwrap();
    let created = timings.run("create retryable-fork source", || harness.create(&source));
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
        "set -eu; app=$(docker ps --filter label=devcontainer.local_folder=/workspace --format '{{.ID}}'); test -n \"$app\"; image=$(docker inspect -f '{{.Image}}' \"$app\"); mount=/workspace/wt-fork-required-mount; mkdir -p \"$mount\"; docker rm -f wt-fork-restart-failure >/dev/null 2>&1 || true; docker run -d --name wt-fork-restart-failure --mount type=bind,source=\"$mount\",target=/mnt \"$image\" sleep infinity >/dev/null; rm -rf \"$mount\"; test \"$(docker inspect -f '{{.State.Running}}' wt-fork-restart-failure)\" = true",
        "inject post-pivot container restart failure",
    );
    let after_error = timings.run("exercise post-pivot fork failure", || {
        call_api_result(
            harness.temp.path(),
            &harness.server_config_path,
            Operation::Fork(ForkInstance {
                source: source.clone(),
                name: destination.clone(),
            }),
        )
        .unwrap_err()
    });
    assert!(
        after_error.contains("restart fork containers"),
        "unexpected post-pivot failure: {after_error}"
    );
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 2
    );
    let instances = harness.sync_inventory();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].name, source);
    assert_eq!(instances[0].status, InstanceStatus::Running);
    run_guest(
        &harness,
        &source,
        "set -eu; mkdir -p /workspace/wt-fork-required-mount; test -n \"$(docker ps -q)\"",
        "repair source container after post-pivot failure",
    );

    let retry = timings.run("retry fork after post-pivot failure", || {
        harness.fork(&source, &destination)
    });
    assert_eq!(retry.status, InstanceStatus::Running);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 4
    );
    harness.sync_inventory();
    run_guest(
        &harness,
        &source,
        "test -n \"$(docker ps -q)\"",
        "verify source after successful retry",
    );
    run_guest(
        &harness,
        &destination,
        "test -n \"$(docker ps -q)\"",
        "verify retried fork container",
    );
    harness.delete(&destination);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes + 3
    );
    harness.delete(&source);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        harness.initial_disk_nodes
    );
}
