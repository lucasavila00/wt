use super::*;
use std::path::Path;

const MINIMAL_HOST_USER_DATA: &str = r#"#cloud-config
runcmd:
  - [sh, -c, "printf 'ready\n' > /var/lib/wt-kvm-fast-ready"]
"#;

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn fast_shared_folder_lifecycle() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new("fast");
    let harness = KvmHarness::new_fast(&mut timings);
    let dev_name = unique_name("fast-dev");
    let host_name = unique_name("fast-host");

    let created_dev = timings.run("create devcontainer world", || harness.create(&dev_name));
    assert_eq!(created_dev.status, InstanceStatus::Setup);
    let running_dev = timings.run("finish minimal devcontainer setup", || {
        harness.finish_setup(&dev_name)
    });
    assert_eq!(running_dev.status, InstanceStatus::Running);

    let created_host = timings.run("create minimal host world", || {
        harness.create_host(&host_name, MINIMAL_HOST_USER_DATA)
    });
    assert_eq!(created_host.status, InstanceStatus::Setup);
    harness.sync_inventory();
    let mut host_setup = spawn_host_byobu(&harness, &host_name);
    let running_host = timings.run("finish minimal host setup", || {
        wait_for_host_status(
            &harness,
            &host_name,
            &mut host_setup,
            InstanceStatus::Running,
        )
    });
    let _ = host_setup.kill();
    let _ = host_setup.wait();

    let marker = format!("wt-kvm-fast-{}", created_dev.id.simple());
    let codex_source = shared_source(&harness, ".codex/sessions").join(&marker);
    let claude_source = shared_source(&harness, ".claude/projects").join(&marker);
    run_guest(
        &harness,
        &dev_name,
        &format!(
            "set -eu; test \"$(id -u)\" = 1001; test \"$(id -g)\" = 1001; \
             test \"$(findmnt -n -o SOURCE --mountpoint /home/wt/.codex/sessions)\" = wt-shared-0; \
             test \"$(findmnt -n -o FSTYPE --mountpoint /home/wt/.codex/sessions)\" = virtiofs; \
             printf 'from-dev\n' > /home/wt/.codex/sessions/{marker}; sync"
        ),
        "verify devcontainer VM shared mounts",
    );
    run_host(
        &harness,
        &host_name,
        &format!(
            "set -eu; test -f /var/lib/wt-kvm-fast-ready; \
             test \"$(findmnt -n -o SOURCE --mountpoint /home/wt/.claude/projects)\" = wt-shared-1; \
             test \"$(cat /home/wt/.codex/sessions/{marker})\" = from-dev; \
             printf 'from-host\n' > /home/wt/.claude/projects/{marker}; sync"
        ),
        "verify host VM shared mounts",
    );
    app(
        &harness,
        &dev_name,
        &format!(
            "test \"$(id -un)\" = wt && \
             test \"$(cat /home/wt/.codex/sessions/{marker})\" = from-dev && \
             test \"$(cat /home/wt/.claude/projects/{marker})\" = from-host"
        ),
        "verify Docker Compose shared-folder binds",
    );

    timings.run("restart devcontainer world", || {
        harness.stop(&running_dev);
        harness.sync_inventory();
        assert_eq!(harness.start(&dev_name).status, InstanceStatus::Running);
        harness.sync_inventory();
    });
    app(
        &harness,
        &dev_name,
        &format!(
            "test \"$(cat /home/wt/.codex/sessions/{marker})\" = from-dev && \
             test \"$(cat /home/wt/.claude/projects/{marker})\" = from-host"
        ),
        "verify Compose binds after restart",
    );

    timings.run("restart host world", || {
        harness.stop(&running_host);
        harness.sync_inventory();
        assert_eq!(harness.start(&host_name).status, InstanceStatus::Running);
        harness.sync_inventory();
    });
    run_host(
        &harness,
        &host_name,
        &format!(
            "test \"$(cat /home/wt/.codex/sessions/{marker})\" = from-dev && \
             test \"$(cat /home/wt/.claude/projects/{marker})\" = from-host"
        ),
        "verify shared mounts after host restart",
    );

    harness.delete(&dev_name);
    run_host(
        &harness,
        &host_name,
        &format!("test \"$(cat /home/wt/.codex/sessions/{marker})\" = from-dev"),
        "verify shared data after deleting the devcontainer world",
    );
    run(
        cmd!("sudo", "-n", "test", "-f", &codex_source),
        "verify Codex data persists on the server",
    );
    run(
        cmd!("sudo", "-n", "test", "-f", &claude_source),
        "verify Claude data persists on the server",
    );
    run_host(
        &harness,
        &host_name,
        &format!(
            "rm -f /home/wt/.codex/sessions/{marker} \
             /home/wt/.claude/projects/{marker}"
        ),
        "remove fast KVM test markers",
    );
    harness.delete(&host_name);
}

fn shared_source(harness: &KvmHarness, target: &str) -> std::path::PathBuf {
    harness
        .config
        .shared_folders
        .iter()
        .find(|folder| folder.target == Path::new(target))
        .unwrap()
        .source
        .clone()
}

fn app(harness: &KvmHarness, name: &InstanceName, command: &str, action: &str) {
    let output = cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}-vs"),
        format!("cd /workspaces/wt && {command}"),
    )
    .env_remove("SSH_AUTH_SOCK")
    .output()
    .unwrap();
    ensure_success(action, &output).unwrap();
}
