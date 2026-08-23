use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::Stdio;

#[test]
#[ignore = "requires installed KVM image and host integration"]
fn host_world_lifecycle() {
    let _lock = acquire_kvm_test_lock();
    let mut timings = Timings::new();
    let name = unique_name("host");
    let mut harness = KvmHarness::new(&mut timings);
    let codex_auth_sha256 = assert_server_codex_auth_export(&harness.config);
    let codex_sessions = CodexSessionFixture::new(&name, &harness.config);

    let created = timings.run("create host", || harness.create(&name));
    assert_eq!(created.status, InstanceStatus::Running);
    assert!(created.ssh.is_some());
    let grant_token = harness.grant_token_for(created.id);
    harness.sync_inventory();
    assert_eq!(
        count_disks(&harness.config.libvirt.worlds_dir),
        harness.initial_disks + 1
    );

    run_guest(
        &harness,
        &name,
        concat!(
            "set -eu; ",
            "test \"$(id -un)\" = wt; ",
            "test \"$(id -u)\" = 1001; ",
            "test \"$(id -g)\" = 1001; ",
            "test \"$(git config --get user.name)\" = 'WT E2E'; ",
            "test \"$(git config --get user.email)\" = wt@example.invalid; ",
            "test -S /run/wt-agent-tool-gateway/gateway.sock; ",
            "systemctl is-active --quiet wt-agent-tool-gateway-relay.service; ",
            "test ! -e /workspace; ",
            ". /home/wt/.nvm/nvm.sh; ",
            "command -v cargo rustc go python nvm node npm uv docker; ",
            "docker compose version >/dev/null; ",
            "command -v git codex diffo wt-tools; ",
            "codex --version >/dev/null"
        ),
        "verify golden guest image",
    );

    let mut byobu = cmd!(
        "ssh",
        "-tt",
        "-F",
        harness.temp.path().join(".ssh/config"),
        format!("local.{name}"),
    )
    .env_remove("SSH_AUTH_SOCK")
    .env("TERM", "xterm-ghostty")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let output = guest_command(&harness, &name, "tmux has-session -t wt-host")
            .output()
            .unwrap();
        if output.status.success() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Byobu session did not start"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_terminal_stack(&harness, &name);
    let _ = byobu.kill();
    let _ = byobu.wait();

    run_guest(
        &harness,
        &name,
        concat!(
            "set -eu; ",
            "git clone https://local.test/acme/widget.git /home/wt/project; ",
            "cd /home/wt/project; git switch -c wt/host-gateway; ",
            "printf 'host gateway\\n' >> README.md; git commit -am 'host gateway'; ",
            "git push -u origin wt/host-gateway; ",
            "git switch -c outside; printf 'outside\\n' >> README.md; git commit -am outside; ",
            "! git push origin outside; git tag outside; ! git push origin refs/tags/outside"
        ),
        "use scoped Git gateway",
    );
    assert_ref(&harness.git.repository, "refs/heads/wt/host-gateway", true);
    assert_ref(&harness.git.repository, "refs/heads/outside", false);
    assert_ref(&harness.git.repository, "refs/tags/outside", false);

    run_guest(
        &harness,
        &name,
        &format!(
            "set -eu; test \"$(readlink /home/wt/.codex/auth.json)\" = /run/wt-codex-integration-auth/auth.json; test ! -w /home/wt/.codex/auth.json; umask 077; printf 'from-host\\n' > /home/wt/.codex/sessions/{}",
            codex_sessions.marker
        ),
        "verify Codex integration",
    );
    run_guest(
        &harness,
        &name,
        r#"test "$(wt-tools '{"command":{"action":"report_wt_tool_issue","description":"KVM host fixture"}}')" = '{"type":"confirmation","data":"Recorded wt-tools report for this world."}'"#,
        "use agent tool gateway",
    );
    assert_eq!(
        std::fs::read_to_string(
            std::path::Path::new(harness.config.codex_paths().sessions)
                .join(&codex_sessions.marker)
        )
        .unwrap(),
        "from-host\n"
    );
    let rollout_metadata = std::fs::metadata(
        std::path::Path::new(harness.config.codex_paths().sessions).join(&codex_sessions.marker),
    )
    .unwrap();
    assert_eq!(rollout_metadata.uid(), wt_guest::GUEST_UID);
    assert_eq!(rollout_metadata.gid(), wt_guest::GUEST_GID);
    assert_eq!(rollout_metadata.permissions().mode() & 0o777, 0o600);
    verify_codex_auth_rotation(&harness, &name, &codex_auth_sha256);

    let stopped = timings.run("stop host", || harness.shutdown(&name));
    assert_eq!(stopped.status, InstanceStatus::Stopped);
    let restarted = timings.run("restart host", || harness.start(&name));
    assert_eq!(restarted.status, InstanceStatus::Running);
    harness.sync_inventory();
    run_guest(
        &harness,
        &name,
        "set -eu; git -C /home/wt/project fetch origin; test -S /run/wt-agent-tool-gateway/gateway.sock; systemctl is-active --quiet wt-agent-tool-gateway-relay.service",
        "verify host persistence after restart",
    );

    harness.restart_gateway();
    run_guest(
        &harness,
        &name,
        "git -C /home/wt/project fetch origin",
        "verify gateway reconnection",
    );

    timings.run("delete host", || harness.delete(&name));
    harness.assert_grant_is_revoked(grant_token);
    assert_eq!(
        count_disks(&harness.config.libvirt.worlds_dir),
        harness.initial_disks
    );
}

fn assert_ref(repository: &std::path::Path, reference: &str, expected: bool) {
    let status = std::process::Command::new("git")
        .args(["--git-dir"])
        .arg(repository)
        .args(["show-ref", "--verify", "--quiet", reference])
        .status()
        .unwrap();
    assert_eq!(
        status.success(),
        expected,
        "unexpected state for {reference}"
    );
}
