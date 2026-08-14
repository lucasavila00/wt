use super::*;
use std::path::Path;

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
    harness.sync_inventory();

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
        "printf 'persistent app state\n' > /tmp/wt-kvm-e2e-restart",
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
        "test \"$(cat /tmp/wt-kvm-e2e-restart)\" = 'persistent app state' && git fetch origin",
        "verify app state and Git after KVM restart",
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
