use super::*;

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_fork_cow_identity_and_runtime_matrix() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let temp = &harness.temp;
    let config = &harness.config;
    let git = &harness.git;
    let server_config_path = harness.server_config_path.clone();
    let initial_disk_nodes = harness.initial_disk_nodes;
    let name = unique_name("full");
    let cache_log_since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let source_started = Instant::now();
    let instance = timings.run("create KVM devcontainer world", || harness.create(&name));
    assert_eq!(instance.status, InstanceStatus::Setup);
    assert!(!instance.ssh.as_ref().unwrap().host_keys.is_empty());
    harness.sync_inventory();
    let agent = SshAgent::start(temp.path(), &git.git_key);
    create_failed_setup_pane(temp.path(), &name);
    let mut setup = start_world_setup(temp.path(), &name, &agent);
    let instance = wait_for_running(temp.path(), &server_config_path, &name);
    let source_create_elapsed = source_started.elapsed();
    let _ = setup.kill();
    let _ = setup.wait();
    assert_registry_cache_hit(cache_log_since);

    let Response::Instances { instances } =
        call_api(temp.path(), &server_config_path, Operation::List)
    else {
        panic!("expected list response");
    };
    sync_inventory(&instances).unwrap();
    let ssh_config = temp.path().join(".ssh/config");
    let host_alias = format!("local.{}-host", name.as_str());
    let same_name_error = call_api_result(
        temp.path(),
        &server_config_path,
        Operation::Fork(ForkInstance {
            source: name.clone(),
            name: name.clone(),
        }),
    )
    .unwrap_err();
    assert!(
        same_name_error.contains("fork destination already exists"),
        "unexpected same-name fork error: {same_name_error}"
    );
    let fork_point = cmd!(
        "ssh",
        "-F",
        &ssh_config,
        "-i",
        &git.guest_key,
        &host_alias,
        "set -eu; printf 'fork point\n' > /workspace/wt-fork-point; cat /proc/sys/kernel/random/boot_id > /workspace/wt-source-boot-id; nohup /bin/bash -c 'exec -a wt-fork-memory-marker /bin/sleep 600' </dev/null >/tmp/wt-fork-memory-marker.log 2>&1 & for attempt in 1 2 3 4 5; do pgrep -f '^wt-fork-memory-marker 600$' >/dev/null && break; sleep 1; done; pgrep -f '^wt-fork-memory-marker 600$' >/dev/null; running=$(docker ps -q | head -n 1); test -n \"$running\"; image=$(docker inspect -f '{{.Image}}' \"$running\"); docker rm -f wt-fork-stopped >/dev/null 2>&1 || true; docker create --name wt-fork-stopped \"$image\" sleep infinity >/dev/null; test \"$(docker inspect -f '{{.State.Running}}' wt-fork-stopped)\" = false",
    )
    .output()
    .unwrap();
    ensure_success(
        "prepare uncommitted, process, and stopped-container fork state",
        &fork_point,
    )
    .unwrap();
    let fork_name = InstanceName::parse(format!("{}-fork", name.as_str())).unwrap();
    let fork_started = Instant::now();
    let forked = timings.run("fork running KVM world", || {
        call_api(
            temp.path(),
            &server_config_path,
            Operation::Fork(ForkInstance {
                source: name.clone(),
                name: fork_name.clone(),
            }),
        )
    });
    let fork_elapsed = fork_started.elapsed();
    let Response::Instance {
        instance: fork_instance,
    } = forked
    else {
        panic!("expected fork instance");
    };
    let fork_instance = *fork_instance;
    assert_eq!(fork_instance.status, InstanceStatus::Running);
    assert_eq!(
        (
            fork_instance.vcpus,
            fork_instance.memory_mib,
            fork_instance.disk_gib
        ),
        (instance.vcpus, instance.memory_mib, instance.disk_gib)
    );
    assert_ne!(fork_instance.guest_ip, instance.guest_ip);
    assert_ne!(
        fork_instance.ssh.as_ref().unwrap().host_keys,
        instance.ssh.as_ref().unwrap().host_keys
    );
    assert_ne!(
        fork_instance.app_ssh.as_ref().unwrap().host_keys,
        instance.app_ssh.as_ref().unwrap().host_keys
    );
    assert!(
        fork_elapsed < source_create_elapsed,
        "fork took {fork_elapsed:?}, but source creation took {source_create_elapsed:?}"
    );
    assert_eq!(
        count_disk_nodes(&config.libvirt.worlds_dir),
        initial_disk_nodes + 3
    );
    let collision_error = call_api_result(
        temp.path(),
        &server_config_path,
        Operation::Fork(ForkInstance {
            source: name.clone(),
            name: fork_name.clone(),
        }),
    )
    .unwrap_err();
    assert!(
        collision_error.contains("fork destination already exists"),
        "unexpected existing-destination fork error: {collision_error}"
    );
    let Response::Instance {
        instance: source_after_fork,
    } = call_api(
        temp.path(),
        &server_config_path,
        Operation::Get { name: name.clone() },
    )
    else {
        panic!("expected source instance response");
    };
    assert_eq!(source_after_fork.status, InstanceStatus::Running);

    let Response::Instances { instances } =
        call_api(temp.path(), &server_config_path, Operation::List)
    else {
        panic!("expected list response");
    };
    sync_inventory(&instances).unwrap();
    let fork_host_alias = format!("local.{}-host", fork_name.as_str());
    for (alias, command, action) in [
        (
            host_alias.as_str(),
            "set -eu; test \"$(cat /proc/sys/kernel/random/boot_id)\" = \"$(cat /workspace/wt-source-boot-id)\"; pgrep -f '^wt-fork-memory-marker 600$' >/dev/null; test \"$(docker inspect -f '{{.State.Running}}' wt-fork-stopped)\" = false; test -z \"$(docker ps -q -f name=^/wt-fork-stopped$)\"",
            "verify source continuity and stopped container",
        ),
        (
            fork_host_alias.as_str(),
            "set -eu; test \"$(cat /proc/sys/kernel/random/boot_id)\" != \"$(cat /workspace/wt-source-boot-id)\"; ! pgrep -f '^wt-fork-memory-marker 600$' >/dev/null; test \"$(docker inspect -f '{{.State.Running}}' wt-fork-stopped)\" = false; test -z \"$(docker ps -q -f name=^/wt-fork-stopped$)\"; test -n \"$(docker ps -q | head -n 1)\"",
            "verify process memory was not copied and stopped container stayed stopped",
        ),
    ] {
        let output = cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            alias,
            command,
        )
        .output()
        .unwrap();
        ensure_success(action, &output).unwrap();
    }
    let source_hostname = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "hostname",
        ),
        "read source hostname",
    );
    let fork_hostname = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &fork_host_alias,
            "hostname",
        ),
        "read fork hostname",
    );
    assert_ne!(source_hostname.trim(), fork_hostname.trim());
    let source_session_key = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "cat",
            "/var/lib/wt-app-ssh/session_identity.pub",
        ),
        "read source app session identity",
    );
    let fork_session_key = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &fork_host_alias,
            "cat",
            "/var/lib/wt-app-ssh/session_identity.pub",
        ),
        "read fork app session identity",
    );
    assert_ne!(source_session_key.trim(), fork_session_key.trim());
    for (alias, command, action) in [
        (
            host_alias.as_str(),
            "test \"$(cat /workspace/wt-fork-point)\" = 'fork point' && printf 'source\n' > /workspace/wt-source-only && test ! -e /workspace/wt-fork-only",
            "modify source after fork",
        ),
        (
            fork_host_alias.as_str(),
            "test \"$(cat /workspace/wt-fork-point)\" = 'fork point' && test ! -e /workspace/wt-source-only && printf 'fork\n' > /workspace/wt-fork-only",
            "modify fork after fork point",
        ),
        (
            host_alias.as_str(),
            "test ! -e /workspace/wt-fork-only",
            "verify source and fork disk isolation",
        ),
    ] {
        let output = cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            alias,
            command,
        )
        .output()
        .unwrap();
        ensure_success(action, &output).unwrap();
    }
    let source_machine_id = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "cat",
            "/etc/machine-id",
        ),
        "read source machine ID after fork",
    );
    let fork_machine_id = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &fork_host_alias,
            "cat",
            "/etc/machine-id",
        ),
        "read fork machine ID",
    );
    assert_ne!(source_machine_id.trim(), fork_machine_id.trim());
    let removed_fork = timings.run("remove fork KVM world", || {
        call_api_result(
            temp.path(),
            &server_config_path,
            Operation::Delete {
                name: fork_name.clone(),
            },
        )
    });
    assert!(removed_fork.is_ok(), "remove fork world: {removed_fork:?}");
    assert_eq!(
        count_disk_nodes(&config.libvirt.worlds_dir),
        initial_disk_nodes + 2
    );
    let source_after_delete = cmd!(
        "ssh",
        "-F",
        &ssh_config,
        "-i",
        &git.guest_key,
        &host_alias,
        "test \"$(cat /workspace/wt-source-only)\" = source",
    )
    .output()
    .unwrap();
    ensure_success("verify source after deleting fork", &source_after_delete).unwrap();

    let result = super::guest_lifecycle::verify_source_guest(&harness, &name, &agent, &mut timings);

    let removed = timings.run("remove KVM world", || {
        call_api_result(temp.path(), &server_config_path, Operation::Delete { name })
    });
    assert!(removed.is_ok(), "remove KVM sample world: {removed:?}");
    assert_eq!(
        count_disk_nodes(&config.libvirt.worlds_dir),
        initial_disk_nodes
    );
    result.unwrap();
}
