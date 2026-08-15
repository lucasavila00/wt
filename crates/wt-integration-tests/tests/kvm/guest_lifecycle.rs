use super::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

const HOST_PROJECT_USER_DATA: &str =
    include_str!("../../../../examples/host-world/cloud-init.yaml");

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn agent_git_transport_works_without_provider_credentials() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let mut harness = KvmHarness::new(&mut timings);
    let name = unique_name("git");

    let created = timings.run("create world", || harness.create(&name));
    assert_eq!(created.status, InstanceStatus::Setup);
    harness.sync_inventory();
    let ssh_config = fs::read_to_string(harness.temp.path().join(".ssh/wt/config")).unwrap();
    assert!(!ssh_config.contains("ForwardAgent"));

    let running = timings.run("finish setup without SSH_AUTH_SOCK", || {
        harness.finish_setup(&name)
    });
    assert_eq!(running.status, InstanceStatus::Running);
    let host_name = unique_name("host");
    let progress_log = harness.temp.path().join("api-progress.log");
    let _ = fs::remove_file(&progress_log);
    let host = timings.run("create host world", || {
        harness.create_host(&host_name, HOST_PROJECT_USER_DATA)
    });
    assert_eq!(host.status, InstanceStatus::Running);
    assert_eq!(host.kind(), wt_api::WorldKind::Host);
    let progress = fs::read_to_string(&progress_log).unwrap();
    assert!(
        progress.contains("WT host project development ready"),
        "host cloud-init output was not streamed:\n{progress}"
    );
    let host_machine = harness
        .config
        .libvirt
        .worlds_dir
        .join(format!("wt-{}", host.id.simple()));
    assert_eq!(
        fs::read_to_string(host_machine.join("user-data")).unwrap(),
        HOST_PROJECT_USER_DATA
    );
    let vendor_data = fs::read_to_string(host_machine.join("vendor-data")).unwrap();
    assert!(vendor_data.contains("name: wt"));
    assert!(!vendor_data.contains("wt-project-development"));
    let inventory = harness.sync_inventory();
    assert_eq!(inventory.len(), 2);
    let forwarded_agent = TestSshAgent::start(harness.temp.path(), &harness.git.guest_key);
    run_host_with_agent(
        &harness,
        &host_name,
        forwarded_agent.socket(),
        "test -S \"$SSH_AUTH_SOCK\" && ssh-add -l",
        "verify direct host agent forwarding",
    );
    start_host_byobu(&harness, &host_name, forwarded_agent.socket());
    start_host_byobu(&harness, &host_name, forwarded_agent.socket());
    run_host(
        &harness,
        &host_name,
        concat!(
            "set -eu; ",
            "test \"$(id -un)\" = wt; ",
            "test -f /var/lib/wt-host-example-ready; ",
            "test -d /home/wt/wt/.git; ",
            "sudo -n true; ",
            "test -z \"${SSH_AUTH_SOCK:-}\"; ",
            "test ! -S /run/wt-agent-git/gateway.sock; ",
            "test ! -e /home/wt/.codex/auth.json; ",
            "! command -v docker; ",
            "! command -v devcontainer; ",
            "git --version; rustc --version; cargo clippy --version; rustfmt --version; ",
            "codex --version; pkg-config --exists libvirt; ",
            "cd /home/wt/wt; cargo clippy --workspace --all-targets -- -D warnings; ",
            "test ! -e /workspace; ",
            "test ! -e /usr/local/bin/wt-app-shell; ",
            "test ! -e /usr/local/bin/wt-agent-git-relay; ",
            "test -x /usr/local/bin/wt-host-shell; ",
            "test \"$(tmux -V)\" = 'tmux 3.6b'; ",
            "test \"$(dpkg-query -W -f='${Version}' byobu)\" = '7.15-0ubuntu1'; ",
            "TERM=ghostty tput colors >/dev/null; ",
            "TERM=xterm-ghostty tput colors >/dev/null; ",
            "tmux has-session -t wt-host",
        ),
        "verify raw host world",
    );

    run_guest(
        &harness,
        &name,
        "test -z \"${SSH_AUTH_SOCK:-}\" && test -S /run/wt-agent-git/gateway.sock",
        "verify guest credential isolation",
    );
    app(
        &harness,
        &name,
        "test -z \"${SSH_AUTH_SOCK:-}\" && test -S /run/wt-agent-git/gateway.sock && test \"$(git remote get-url origin)\" = ag::git@local.test:acme/widget.git",
        "verify devcontainer gateway setup",
    );
    app(
        &harness,
        &name,
        concat!(
            "set -eu; ",
            "test \"$(id -un)\" = root; ",
            "rustc --version; cargo clippy --version; rustfmt --version; ",
            "codex --version; pkg-config --exists libvirt; ",
            "test ! -e /root/.codex/auth.json",
        ),
        "verify devcontainer project tools",
    );

    let help = app_output(&harness, &name, "ag-git --help", "read ag-git help");
    assert!(help.contains("explicitly identified Git provider resources"));
    assert!(help.contains("wait mr|run|job ID"));

    run_guest(
        &harness,
        &name,
        "set -eu; old=$(systemctl show -p MainPID --value wt-agent-git-relay.service); kill -KILL \"$old\"; attempt=0; while :; do new=$(systemctl show -p MainPID --value wt-agent-git-relay.service); test \"$new\" != 0 && test \"$new\" != \"$old\" && test -S /run/wt-agent-git/gateway.sock && break; attempt=$((attempt + 1)); test \"$attempt\" -lt 100; sleep 0.1; done",
        "restart the guest Git relay",
    );
    app(
        &harness,
        &name,
        "git fetch origin",
        "fetch through the restarted relay from the existing devcontainer",
    );

    let branch = "wt/fix-login";
    let output = app_command(
        &harness,
        &name,
        &format!(
            "set -eu; git switch -c '{branch}'; printf 'first\\n' >> README.md; git add README.md; git commit -m first; git push"
        ),
    )
    .output()
    .unwrap();
    ensure_success("commit and push through gateway", &output).unwrap();
    let diagnostics = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostics.contains("This is a WT-managed development environment"));
    assert!(diagnostics.contains("Run ag-git --help"));
    assert_ref(
        &harness.git.repository,
        &format!("refs/heads/{branch}"),
        true,
    );
    harness.restart_gateway();
    harness.assert_shared_prefix_is_available();
    app(
        &harness,
        &name,
        "git fetch origin",
        "fetch after gateway restart",
    );

    app(
        &harness,
        &name,
        "set -eu; printf 'second\n' >> README.md; git commit -am second; git push; git reset --hard HEAD^; git push --force",
        "force-push through gateway",
    );
    let local = app_output(&harness, &name, "git rev-parse HEAD", "read local Git head");
    let upstream = cmd!(
        "git",
        "--git-dir",
        &harness.git.repository,
        "rev-parse",
        format!("refs/heads/{branch}")
    )
    .output()
    .unwrap();
    ensure_success("read published Git head", &upstream).unwrap();
    assert_eq!(
        local.trim(),
        String::from_utf8(upstream.stdout).unwrap().trim()
    );

    for provider in ["github", "gitlab"] {
        harness.use_provider_api_fixture(provider, local.trim());
        let status = app_output(
            &harness,
            &name,
            &format!("ag-git list ci commit {}", local.trim()),
            "read explicit CI through provider API fixture",
        );
        assert!(status.contains("No CI resources for the commit"));
        harness.restart_gateway();
    }

    app(
        &harness,
        &name,
        "printf 'persistent app state\n' > /workspaces/workspace/.wt-kvm-e2e-restart && sync",
        "write app state before KVM restart",
    );
    let stopped = timings.run("stop and reconcile world", || {
        harness.stop(&running);
        harness
            .sync_inventory()
            .into_iter()
            .find(|instance| instance.name == name)
            .unwrap()
    });
    assert_eq!(stopped.status, InstanceStatus::Stopped);
    assert_eq!(
        stopped.last_error.as_deref(),
        Some("guest stopped (destroyed)")
    );

    let restarted = timings.run("restart world", || harness.start(&name));
    assert_eq!(restarted.status, InstanceStatus::Running);
    harness.sync_inventory();
    run_guest(
        &harness,
        &name,
        "test -S /run/wt-agent-git/gateway.sock && systemctl is-active --quiet docker.service wt-agent-git-relay.service",
        "verify guest services after KVM restart",
    );
    app(
        &harness,
        &name,
        "test \"$(cat /workspaces/workspace/.wt-kvm-e2e-restart)\" = 'persistent app state' && git fetch origin",
        "verify app state and Git after KVM restart",
    );

    let stopped_host = timings.run("stop and reconcile host world", || {
        harness.stop(&host);
        harness
            .sync_inventory()
            .into_iter()
            .find(|instance| instance.name == host_name)
            .unwrap()
    });
    assert_eq!(stopped_host.status, InstanceStatus::Stopped);
    let restarted_host = timings.run("restart host world", || harness.start(&host_name));
    assert_eq!(restarted_host.status, InstanceStatus::Running);
    harness.sync_inventory();
    run_host(
        &harness,
        &host_name,
        "test -f /var/lib/wt-host-example-ready && test -d /home/wt/wt/.git",
        "verify host state after KVM restart",
    );

    app(
        &harness,
        &name,
        "set -eu; git switch -c wrong; printf 'wrong\n' >> README.md; git commit -am wrong; ! git push origin wrong; git tag bad-tag; ! git push origin bad-tag",
        "reject branches and tags outside the world scope",
    );
    assert_ref(&harness.git.repository, "refs/heads/wrong", false);
    assert_ref(&harness.git.repository, "refs/tags/bad-tag", false);

    app(
        &harness,
        &name,
        &format!("git push origin --delete '{branch}'"),
        "delete published branch through gateway",
    );
    assert_ref(
        &harness.git.repository,
        &format!("refs/heads/{branch}"),
        false,
    );
    let token = harness.grant_token();
    harness.delete(&name);
    harness.assert_grant_is_revoked(token);
    harness.delete(&host_name);

    let broken_name = unique_name("host-no-ssh");
    let disks_before = count_disk_nodes(&harness.config.libvirt.worlds_dir);
    let _ = fs::remove_file(&progress_log);
    let error = timings.run("reject host without WT SSH", || {
        harness
            .create_host_result(
                &broken_name,
                concat!(
                    "#cloud-config\n",
                    "runcmd:\n",
                    "  - |\n",
                    "    echo 'broken host stdout'\n",
                    "    echo 'broken host stderr' >&2\n",
                    "    rm -f /home/wt/.ssh/authorized_keys\n",
                ),
            )
            .unwrap_err()
    });
    assert!(error.contains("SSH login readiness failed"), "{error}");
    let progress = fs::read_to_string(&progress_log).unwrap();
    let stdout = progress.find("broken host stdout").unwrap();
    let stderr = progress.find("broken host stderr").unwrap();
    assert!(
        stdout < stderr,
        "cloud-init output was reordered:\n{progress}"
    );
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        disks_before + 1
    );
    let failed = harness
        .sync_inventory()
        .into_iter()
        .find(|instance| instance.name == broken_name)
        .expect("failed host is missing from inventory");
    assert_eq!(failed.status, InstanceStatus::Error);
    assert!(
        failed
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("SSH login readiness failed")),
        "failed host has no useful error: {failed:?}"
    );
    harness.delete(&broken_name);
    assert_eq!(
        count_disk_nodes(&harness.config.libvirt.worlds_dir),
        disks_before
    );
}

fn run_host(harness: &KvmHarness, name: &InstanceName, command: &str, action: &str) {
    let mut command_process = host_command(harness, name, command);
    command_process.env_remove("SSH_AUTH_SOCK");
    let output = command_process.output().unwrap();
    ensure_success(action, &output).unwrap();
}

fn run_host_with_agent(
    harness: &KvmHarness,
    name: &InstanceName,
    agent_socket: &Path,
    command: &str,
    action: &str,
) {
    let output = host_command(harness, name, command)
        .env("SSH_AUTH_SOCK", agent_socket)
        .output()
        .unwrap();
    ensure_success(action, &output).unwrap();
}

fn host_command(harness: &KvmHarness, name: &InstanceName, command: &str) -> std::process::Command {
    cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}-vs"),
        command,
    )
}

fn start_host_byobu(harness: &KvmHarness, name: &InstanceName, agent_socket: &Path) {
    let mut command = cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}"),
    );
    command
        .env("SSH_AUTH_SOCK", agent_socket)
        .env("TERM", "xterm-ghostty")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let mut probe = cmd!(
            "ssh",
            "-F",
            harness.temp.path().join(".ssh/config"),
            "-i",
            &harness.git.guest_key,
            format!("local.{name}-vs"),
            concat!(
                "test \"$(tmux show-environment -t wt-host SSH_AUTH_SOCK)\" = ",
                "'SSH_AUTH_SOCK=/home/wt/.local/state/wt/ssh-agent' && ",
                "SSH_AUTH_SOCK=/home/wt/.local/state/wt/ssh-agent ssh-add -l",
            ),
        );
        probe.env_remove("SSH_AUTH_SOCK");
        let output = probe.output().unwrap();
        if output.status.success() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "host Byobu session did not start: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
}

struct TestSshAgent {
    child: Child,
    socket: PathBuf,
}

impl TestSshAgent {
    fn start(root: &Path, key: &Path) -> Self {
        let socket = root.join("forwarded-agent.sock");
        let mut child = cmd!("ssh-agent", "-D", "-a", &socket)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !socket.exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "disposable SSH agent exited before creating its socket"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "disposable SSH agent socket did not appear"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = cmd!("ssh-add", key)
            .env("SSH_AUTH_SOCK", &socket)
            .output()
            .unwrap();
        ensure_success("add the disposable forwarded identity", &output).unwrap();
        Self { child, socket }
    }

    fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for TestSshAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn app(harness: &KvmHarness, name: &InstanceName, command: &str, action: &str) {
    let output = app_command(harness, name, command).output().unwrap();
    ensure_success(action, &output).unwrap();
}

fn app_output(harness: &KvmHarness, name: &InstanceName, command: &str, action: &str) -> String {
    let output = app_command(harness, name, command).output().unwrap();
    ensure_success(action, &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn app_command(harness: &KvmHarness, name: &InstanceName, command: &str) -> std::process::Command {
    let mut command_process = cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}-vs"),
        format!("cd /workspaces/workspace && {command}"),
    );
    command_process.env_remove("SSH_AUTH_SOCK");
    command_process
}

fn assert_ref(repository: &Path, reference: &str, exists: bool) {
    let output = cmd!(
        "git",
        "--git-dir",
        repository,
        "show-ref",
        "--verify",
        reference
    )
    .output()
    .unwrap();
    assert_eq!(output.status.success(), exists, "{reference}");
}
