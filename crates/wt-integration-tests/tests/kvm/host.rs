use super::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

pub(crate) fn wait_for_live_host_output(
    harness: &KvmHarness,
    name: &InstanceName,
    setup: &mut Child,
    marker: &str,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let output = host_command(harness, name, "tmux capture-pane -p -S - -t wt-host:0.0")
            .output()
            .unwrap();
        if output.status.success() && String::from_utf8_lossy(&output.stdout).contains(marker) {
            let instance = harness
                .sync_inventory()
                .into_iter()
                .find(|instance| instance.name == *name)
                .unwrap();
            assert_eq!(
                instance.status,
                InstanceStatus::Setup,
                "host output was only visible after setup completed"
            );
            return;
        }
        if let Some(status) = setup.try_wait().unwrap() {
            panic!("host setup SSH exited before live output: {status}");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for live host cloud-init output\n\
             capture status: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

pub(crate) fn spawn_host_byobu(harness: &KvmHarness, name: &InstanceName) -> Child {
    let mut command = cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}"),
    );
    command
        .env_remove("SSH_AUTH_SOCK")
        .env("TERM", "xterm-ghostty")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
}

pub(crate) fn wait_for_host_status(
    harness: &KvmHarness,
    name: &InstanceName,
    setup: &mut Child,
    expected: InstanceStatus,
) -> wt_api::Instance {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
    loop {
        let instance = harness
            .sync_inventory()
            .into_iter()
            .find(|instance| instance.name == *name)
            .unwrap();
        if instance.status == expected {
            return instance;
        }
        if let Some(status) = setup.try_wait().unwrap() {
            panic!(
                "host setup SSH exited before {expected}: {status}\n{}",
                capture_host_pane(harness, name)
            );
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for host to become {expected}"
        );
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

pub(crate) fn capture_host_pane(harness: &KvmHarness, name: &InstanceName) -> String {
    let output = host_command(harness, name, "tmux capture-pane -p -S - -t wt-host:0.0")
        .output()
        .unwrap();
    ensure_success("capture host Byobu output", &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

pub(crate) fn run_host(harness: &KvmHarness, name: &InstanceName, command: &str, action: &str) {
    let mut command_process = host_command(harness, name, command);
    command_process.env_remove("SSH_AUTH_SOCK");
    let output = command_process.output().unwrap();
    ensure_success(action, &output).unwrap();
}

pub(crate) fn run_host_with_forwarded_agent(
    harness: &KvmHarness,
    name: &InstanceName,
    agent_socket: &Path,
    command: &str,
    action: &str,
) {
    let output = cmd!(
        "ssh",
        "-A",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}-vs"),
        command,
    )
    .env("SSH_AUTH_SOCK", agent_socket)
    .output()
    .unwrap();
    ensure_success(action, &output).unwrap();
}

pub(crate) fn host_command(
    harness: &KvmHarness,
    name: &InstanceName,
    command: &str,
) -> std::process::Command {
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

pub(crate) fn start_host_byobu(harness: &KvmHarness, name: &InstanceName) {
    let mut command = cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}"),
    );
    command
        .env_remove("SSH_AUTH_SOCK")
        .env("TERM", "xterm-ghostty")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let mut probe = host_command(harness, name, "tmux has-session -t wt-host");
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

pub(crate) struct TestSshAgent {
    child: Child,
    socket: PathBuf,
}

impl TestSshAgent {
    pub(crate) fn start(root: &Path, key: &Path) -> Self {
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

    pub(crate) fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for TestSshAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
