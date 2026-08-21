use super::*;
use std::path::Path;

const HOST_DEFAULT_USER_DATA: &str = include_str!("../../../../../assets/client/cloud-init.yaml");

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn agent_tools_transport_works_without_provider_credentials() {
    let _serial = KVM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut timings = Timings::new();
    let mut harness = KvmHarness::new(&mut timings);
    let codex_auth_sha256 = assert_server_codex_auth_export();
    let name = unique_name("git");
    let codex_session_fixture = CodexSessionFixture::new(&name);

    let disks_before_rejection = count_disks(&harness.config.libvirt.worlds_dir);
    let mut reserved_field_errors = Vec::new();
    for (index, field) in ["ssh_keys", "ssh_deletekeys", "output"]
        .into_iter()
        .enumerate()
    {
        let rejected_name = unique_name(&format!("host-reject-{index}"));
        let error = harness
            .create_host_result(
                &rejected_name,
                &format!("#cloud-config\n{field}: forbidden\n"),
            )
            .unwrap_err();
        reserved_field_errors.push(error);
    }
    insta::assert_snapshot!(reserved_field_errors.join("\n"), @r###"
    0: cloud-init user-data cannot set top-level ssh_keys; WT owns host identity, setup stages, and output
    0: cloud-init user-data cannot set top-level ssh_deletekeys; WT owns host identity, setup stages, and output
    0: cloud-init user-data cannot set top-level output; WT owns host identity, setup stages, and output
    "###);
    assert_eq!(
        count_disks(&harness.config.libvirt.worlds_dir),
        disks_before_rejection
    );
    assert!(harness.sync_inventory().is_empty());

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
    let created_host = timings.run("prepare host world", || {
        harness.create_host(&host_name, HOST_DEFAULT_USER_DATA)
    });
    assert_eq!(created_host.status, InstanceStatus::Setup);
    assert_eq!(created_host.kind(), wt_control_protocol::WorldKind::Host);
    let host_machine = harness
        .config
        .libvirt
        .worlds_dir
        .join(format!("wt-{}", created_host.id.simple()));
    insta::assert_snapshot!(
        fs::read_to_string(host_machine.join("user-data")).unwrap(),
        @"#cloud-config"
    );
    insta::assert_snapshot!(
        fs::read_to_string(host_machine.join("vendor-data")).unwrap(),
        @"#cloud-config"
    );
    let inventory = harness.sync_inventory();
    assert_eq!(inventory.len(), 2);
    let shared_marker = &codex_session_fixture.marker;
    let codex_source = Path::new(wt_server::CODEX_SESSIONS_PATH).join(shared_marker);
    run_guest(
        &harness,
        &name,
        &format!(
            "set -eu; test \"$(id -u)\" = 1001; test \"$(id -g)\" = 1001; \
             test -x /usr/local/bin/wt-codex-integration; \
             test \"$(findmnt -n -o SOURCE --mountpoint /home/wt/.codex/sessions)\" = wt-codex-integration-sessions; \
             test \"$(findmnt -n -o FSTYPE --mountpoint /home/wt/.codex/sessions)\" = virtiofs; \
             test \"$(findmnt -n -o SOURCE --mountpoint /run/wt-codex-integration-auth)\" = wt-codex-integration-auth; \
             test \"$(readlink /home/wt/.codex/auth.json)\" = /run/wt-codex-integration-auth/auth.json; \
             test \"$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}')\" = {codex_auth_sha256}; \
             test ! -w /home/wt/.codex/auth.json; \
             test -w \"$(dirname /home/wt/.codex/sessions/{shared_marker})\"; \
             printf 'from-devcontainer-vm\\n' > /home/wt/.codex/sessions/{shared_marker}; sync"
        ),
        "write a shared marker through the devcontainer VM",
    );
    assert_eq!(
        fs::read_to_string(&codex_source).unwrap(),
        "from-devcontainer-vm\n"
    );
    fs::write(&codex_source, "from-server\n").unwrap();
    run_host(
        &harness,
        &host_name,
        &format!(
            "set -eu; test \"$(id -u)\" = 1001; test \"$(id -g)\" = 1001; \
             test \"$(findmnt -n -o SOURCE --mountpoint /home/wt/.codex/sessions)\" = wt-codex-integration-sessions; \
             test \"$(findmnt -n -o SOURCE --mountpoint /run/wt-codex-integration-auth)\" = wt-codex-integration-auth; \
             test \"$(readlink /home/wt/.codex/auth.json)\" = /run/wt-codex-integration-auth/auth.json; \
             test \"$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}')\" = {codex_auth_sha256}; \
             test ! -w /home/wt/.codex/auth.json; \
             test -w \"$(dirname /home/wt/.codex/sessions/{shared_marker})\"; \
             test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-server; \
             printf 'from-host-vm\\n' > /home/wt/.codex/sessions/{shared_marker}; sync"
        ),
        "read and write shared markers through the host VM",
    );
    run_guest(
        &harness,
        &name,
        &format!("test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-host-vm"),
        "read the host marker through the devcontainer VM",
    );
    app(
        &harness,
        &name,
        &format!(
            "set -eu; \
             test \"$(readlink /home/wt/.codex/sessions)\" = /var/lib/wt-codex-integration-sessions; \
             test \"$(readlink /home/wt/.codex/auth.json)\" = /var/lib/wt-codex-integration-auth/auth.json; \
             test \"$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}')\" = {codex_auth_sha256}; \
             test ! -w /home/wt/.codex/auth.json; \
             test -x /usr/local/bin/wt-codex-integration; test -x /usr/local/bin/.codex.wt-real; \
             cmp /usr/local/bin/codex /usr/local/bin/wt-codex-integration; \
             test -w \"$(dirname /home/wt/.codex/sessions/{shared_marker})\"; \
             test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-host-vm; \
             printf 'from-app\n' > /home/wt/.codex/sessions/{shared_marker}; sync"
        ),
        "verify automatic Codex integration in the devcontainer",
    );
    assert_eq!(fs::read_to_string(&codex_source).unwrap(), "from-app\n");
    run_host(
        &harness,
        &host_name,
        &format!(
            "test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-app && \
             printf 'from-host-vm\n' > /home/wt/.codex/sessions/{shared_marker} && sync"
        ),
        "read the app marker and restore the host marker",
    );
    verify_codex_auth_rotation(&harness, &name, &host_name, &codex_auth_sha256);
    run_host(
        &harness,
        &host_name,
        concat!(
            "test \"$(sudo stat -c '%U:%G %a' /var/lib/wt-host)\" = 'root:root 711'; ",
            "test \"$(sudo stat -c '%U:%G %a' /var/lib/wt-host/user-data)\" = ",
            "'root:root 600'; ",
            "test -x /var/lib/wt-host; ",
            "test ! -r /var/lib/wt-host/user-data",
        ),
        "verify staged host user-data permissions",
    );
    let staged_user_data =
        host_command(&harness, &host_name, "sudo cat /var/lib/wt-host/user-data")
            .output()
            .unwrap();
    ensure_success("read staged host user-data", &staged_user_data).unwrap();
    assert_eq!(staged_user_data.stdout, HOST_DEFAULT_USER_DATA.as_bytes());
    run_host(
        &harness,
        &host_name,
        "test ! -e /var/lib/wt-host/started",
        "verify direct host SSH does not start setup",
    );
    let still_setup = harness
        .sync_inventory()
        .into_iter()
        .find(|instance| instance.name == host_name)
        .unwrap();
    assert_eq!(still_setup.status, InstanceStatus::Setup);

    harness.stop(&created_host);
    let stopped_setup = harness
        .sync_inventory()
        .into_iter()
        .find(|instance| instance.name == host_name)
        .unwrap();
    assert_eq!(stopped_setup.status, InstanceStatus::Stopped);
    let restarted_setup = harness.start(&host_name);
    assert_eq!(restarted_setup.status, InstanceStatus::Setup);
    harness.sync_inventory();
    run_host(
        &harness,
        &host_name,
        "test -z \"${SSH_AUTH_SOCK:-}\"",
        "verify host agent forwarding is not automatic",
    );
    {
        let forwarded_agent = TestSshAgent::start(harness.temp.path(), &harness.git.guest_key);
        run_host_with_forwarded_agent(
            &harness,
            &host_name,
            forwarded_agent.socket(),
            "test -S \"$SSH_AUTH_SOCK\" && ssh-add -l",
            "verify explicit direct host agent forwarding",
        );
    }
    let mut host_setup = spawn_host_byobu(&harness, &host_name);
    wait_for_live_host_output(
        &harness,
        &host_name,
        &mut host_setup,
        "WT host cloud-init: init",
    );
    let host = timings.run("run host cloud-init in Byobu", || {
        wait_for_host_status(
            &harness,
            &host_name,
            &mut host_setup,
            InstanceStatus::Running,
        )
    });
    run_host(
        &harness,
        &host_name,
        concat!(
            "for attempt in $(seq 1 50); do ",
            "test \"$(tmux display-message -p -t wt-host:0.0 ",
            "'#{pane_dead} #{pane_current_command}')\" = '0 bash' && exit 0; ",
            "sleep 0.2; ",
            "done; ",
            "tmux list-panes -t wt-host -F '#{pane_dead} #{pane_current_command}'; ",
            "exit 1",
        ),
        "wait for host setup pane to become a login shell",
    );
    let host_pane = capture_host_pane(&harness, &host_name);
    assert!(
        host_pane.contains("WT host cloud-init: init")
            && host_pane.contains("WT host cloud-init complete."),
        "host cloud-init output was not preserved in Byobu:\n{host_pane}"
    );
    let _ = host_setup.kill();
    let _ = host_setup.wait();
    start_host_byobu(&harness, &host_name);
    start_host_byobu(&harness, &host_name);
    assert_shared_terminal_stack(&harness, &name, &host_name);
    run_host(
        &harness,
        &host_name,
        concat!(
            "set -eu; ",
            "test \"$(id -un)\" = wt; ",
            "test \"$(git config --global --get user.name)\" = 'WT E2E'; ",
            "test \"$(git config --global --get user.email)\" = wt@example.invalid; ",
            "sudo -n true; ",
            "test -z \"${SSH_AUTH_SOCK:-}\"; ",
            "test -S /run/wt-agent-tool-gateway/gateway.sock; ",
            "test -s /home/wt/.codex/auth.json; test ! -w /home/wt/.codex/auth.json; ",
            "! command -v docker; ",
            "! command -v devcontainer; ",
            "git --version; curl --version; codex --version; command -v diffo; ",
            "test \"$(readlink /usr/local/bin/codex)\" = /usr/local/bin/wt-codex-integration; ",
            "test \"$(readlink /usr/local/bin/.codex.wt-real)\" = /home/wt/.local/bin/codex; ",
            "sudo -n wt-codex-integration uninstall; ",
            "test \"$(readlink /usr/local/bin/codex)\" = /home/wt/.local/bin/codex; ",
            "sudo -n wt-codex-integration install; ",
            "test ! -e /workspace; ",
            "test ! -e /usr/local/bin/wt-app-shell; ",
            "test -x /usr/local/bin/wt-agent-tool-gateway-relay; ",
            "test -x /usr/local/bin/git-remote-wt-agent; ",
            "test -x /usr/local/bin/wt-tools; ",
            "systemctl is-active --quiet wt-agent-tool-gateway-relay.service; ",
            "test -x /usr/local/bin/wt-host-shell; ",
            "tmux has-session -t wt-host",
        ),
        "verify raw host world",
    );
    run_host(
        &harness,
        &host_name,
        concat!(
            "set -eu; ",
            "git clone https://local.test/acme/widget.git /home/wt/gateway-check; ",
            "cd /home/wt/gateway-check; ",
            "git switch -c wt/host-gateway; ",
            "printf 'host gateway\n' >> README.md; ",
            "git commit -am 'host gateway'; ",
            "git push -u origin wt/host-gateway; ",
            "wt-tools --help >/dev/null; ",
            "git switch -c outside-host; ",
            "printf 'outside\n' >> README.md; ",
            "git commit -am outside; ",
            "! git push origin refs/heads/outside-host; ",
            "git tag outside-host; ",
            "! git push origin refs/tags/outside-host",
        ),
        "use the host Git gateway with normal URLs",
    );
    assert_ref(&harness.git.repository, "refs/heads/wt/host-gateway", true);
    assert_ref(&harness.git.repository, "refs/heads/outside-host", false);
    assert_ref(&harness.git.repository, "refs/tags/outside-host", false);
    run_host(
        &harness,
        &host_name,
        "set -eu; old=$(systemctl show -p MainPID --value wt-agent-tool-gateway-relay.service); sudo kill -KILL \"$old\"; attempt=0; while :; do new=$(systemctl show -p MainPID --value wt-agent-tool-gateway-relay.service); test \"$new\" != 0 && test \"$new\" != \"$old\" && test -S /run/wt-agent-tool-gateway/gateway.sock && break; attempt=$((attempt + 1)); test \"$attempt\" -lt 100; sleep 0.1; done; git -C /home/wt/gateway-check fetch origin",
        "restart the host Git relay",
    );

    run_guest(
        &harness,
        &name,
        concat!(
            "set -eu; ",
            "test -z \"${SSH_AUTH_SOCK:-}\"; ",
            "test -S /run/wt-agent-tool-gateway/gateway.sock",
        ),
        "verify guest credential isolation",
    );
    run_guest(
        &harness,
        &name,
        "test \"$(git -C /workspace config --get user.name)\" = 'WT E2E' && test \"$(git -C /workspace config --get user.email)\" = wt@example.invalid",
        "verify guest Git author transfer",
    );
    app(
        &harness,
        &name,
        "test -z \"${SSH_AUTH_SOCK:-}\" && test -S /run/wt-agent-tool-gateway/gateway.sock && test \"$(git remote get-url origin)\" = wt-agent::git@local.test:acme/widget.git",
        "verify devcontainer gateway setup",
    );
    app(
        &harness,
        &name,
        concat!(
            "set -eu; ",
            "test \"$(id -un)\" = wt; ",
            "rustc --version; cargo clippy --version; rustfmt --version; ",
            "codex --version; pkg-config --exists libvirt",
        ),
        "verify devcontainer project tools",
    );
    app(
        &harness,
        &name,
        "test \"$(git config --get user.name)\" = 'WT E2E' && test \"$(git config --get user.email)\" = wt@example.invalid",
        "verify devcontainer Git author transfer",
    );
    app(
        &harness,
        &name,
        &format!("test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-host-vm"),
        "verify automatic Codex session mount",
    );

    let help = app_output(&harness, &name, "wt-tools --help", "read wt-tools help");
    insta::assert_snapshot!(help, @r###"
    wt-tools reads and changes explicitly identified Git provider resources and records
    feedback about wt-tools itself. It accepts exactly one JSON command object and
    rejects unknown fields.

    USAGE:
    wt-tools '<JSON>'

    TYPESCRIPT COMMAND TYPE:
    type WtToolsCommand =
    | { action: "show_mr"; mr: number }
    | { action: "show_mr_for_branch"; branch: string }
    | { action: "show_run"; run: number }
    | { action: "show_job"; job: number }
    | { action: "list_threads"; mr: number }
    | { action: "list_ci"; commit: string }
    | { action: "list_jobs"; run: number }
    | { action: "log_job"; job: number }
    | { action: "wait_mr"; mr: number; timeout_seconds?: number }
    | { action: "wait_run"; run: number; timeout_seconds?: number }
    | { action: "wait_job"; job: number; timeout_seconds?: number }
    | { action: "open_mr"; head: string; base: string; draft?: boolean }
    | { action: "set_mr"; mr: number; state: "ready" | "draft" | "open" | "closed" }
    | { action: "edit_mr"; mr: number; title?: string; body?: string }
    | { action: "comment_mr"; mr: number; body: string }
    | { action: "reply_thread"; mr: number; thread: string; body: string }
    | { action: "set_thread"; mr: number; thread: string; resolved: boolean }
    | { action: "retry_job"; job: number }
    | { action: "cancel_job"; job: number }
    | { action: "cancel_run"; run: number }
    | { action: "report_wt_tool_bug"; description: string }
    | { action: "report_wt_tool_issue"; description: string }
    | { action: "suggest_wt_tool_improvement"; description: string }
    | { action: "request_wt_tool_feature"; description: string };

    EXAMPLE:
    wt-tools '{"action":"show_mr_for_branch","branch":"wt/fix-login"}'

    `show_mr_for_branch` returns the single open MR from the named branch to the
    gateway grant's base branch. It fails when there is no match or multiple matches.

    The four wt-tools reporting actions store feedback against this authenticated world
    without contacting the Git provider.

    The provider and project come from this world's gateway grant. Every other
    resource is explicit. IDs must be positive integers. Commit values must be 7 to
    64 hexadecimal characters. Use normal Git for commits, fetches, pulls, and pushes.
    "###);

    run_guest(
        &harness,
        &name,
        "set -eu; old=$(systemctl show -p MainPID --value wt-agent-tool-gateway-relay.service); kill -KILL \"$old\"; attempt=0; while :; do new=$(systemctl show -p MainPID --value wt-agent-tool-gateway-relay.service); test \"$new\" != 0 && test \"$new\" != \"$old\" && test -S /run/wt-agent-tool-gateway/gateway.sock && break; attempt=$((attempt + 1)); test \"$attempt\" -lt 100; sleep 0.1; done",
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
    let gateway_diagnostics = diagnostics
        .lines()
        .filter_map(|line| line.strip_prefix("remote: "))
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(gateway_diagnostics, @"
    This is a WT-managed development environment for a coding agent.
    The gateway does not expose developer SSH keys or provider credentials.
    Do not look for credentials or use gh or glab.
    WT gives you read access to every repository available to this gateway.
    This Git operation is for project acme/widget.
    Use normal Git for commits, fetches, pulls, and pushes.
    Every WT world can write branches under wt/.
    wt-tools uses explicit provider resource types and IDs; it does not infer
    resources from the current checkout.
    Run wt-tools --help to discover every available command.
    Published branch `wt/fix-login`.
    Run `wt-tools --help` for explicit provider commands.
    ");
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

    let mut provider_statuses = Vec::new();
    for provider in ["github", "gitlab"] {
        harness.use_provider_api_fixture(provider, local.trim());
        let status = app_output(
            &harness,
            &name,
            &format!(
                "wt-tools '{{\"action\":\"list_ci\",\"commit\":\"{}\"}}'",
                local.trim()
            ),
            "read explicit CI through provider API fixture",
        );
        provider_statuses.push(format!("{provider}: {}", status.trim_end_matches('\n')));
        harness.restart_gateway();
    }
    insta::assert_snapshot!(provider_statuses.join("\n"), @"
    github: No CI resources for the commit.
    gitlab: No CI resources for the commit.
    ");

    app(
        &harness,
        &name,
        "printf 'persistent app state\n' > /workspaces/wt/.wt-kvm-e2e-restart && sync",
        "write app state before KVM restart",
    );
    run_host(
        &harness,
        &host_name,
        "sync",
        "flush host state before KVM restart",
    );
    let stopped = timings.run("stop world", || harness.shutdown(&name));
    assert_eq!(stopped.status, InstanceStatus::Stopped);
    assert_eq!(
        stopped.last_error.as_deref(),
        Some("guest stopped (requested)")
    );

    let restarted = timings.run("restart world", || harness.start(&name));
    assert_eq!(restarted.status, InstanceStatus::Running);
    harness.sync_inventory();
    run_guest(
        &harness,
        &name,
        &format!(
            "test -S /run/wt-agent-tool-gateway/gateway.sock && \
             systemctl is-active --quiet docker.service wt-agent-tool-gateway-relay.service && \
             test \"$(readlink /home/wt/.codex/auth.json)\" = /run/wt-codex-integration-auth/auth.json && \
             test \"$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}')\" = {codex_auth_sha256} && \
             test ! -w /home/wt/.codex/auth.json && \
             test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-host-vm"
        ),
        "verify guest services after KVM restart",
    );
    app(
        &harness,
        &name,
        &format!(
            "test \"$(cat /workspaces/wt/.wt-kvm-e2e-restart)\" = 'persistent app state' && \
             test \"$(readlink /home/wt/.codex/auth.json)\" = /var/lib/wt-codex-integration-auth/auth.json && \
             test \"$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}')\" = {codex_auth_sha256} && \
             test ! -w /home/wt/.codex/auth.json && git fetch origin"
        ),
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
        &format!(
            "command -v codex diffo && test -S /run/wt-agent-tool-gateway/gateway.sock && \
             systemctl is-active --quiet wt-agent-tool-gateway-relay.service && \
             git -C /home/wt/gateway-check fetch origin && \
             test \"$(readlink /home/wt/.codex/auth.json)\" = /run/wt-codex-integration-auth/auth.json && \
             test \"$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}')\" = {codex_auth_sha256} && \
             test ! -w /home/wt/.codex/auth.json && \
             test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-host-vm"
        ),
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
    run_host(
        &harness,
        &host_name,
        &format!("test \"$(cat /home/wt/.codex/sessions/{shared_marker})\" = from-host-vm"),
        "verify shared data after deleting the devcontainer world",
    );
    run(
        cmd!("sudo", "-n", "test", "-f", &codex_source),
        "verify the Codex marker remains on the server",
    );
    run_host(
        &harness,
        &host_name,
        &format!("rm -f /home/wt/.codex/sessions/{shared_marker}"),
        "remove shared KVM test markers",
    );
    let host_token = harness.grant_token_for(host.id);
    harness.delete(&host_name);
    harness.assert_grant_is_revoked(host_token);

    let broken_name = unique_name("host-cloud-init-failure");
    let disks_before = count_disks(&harness.config.libvirt.worlds_dir);
    let broken = timings.run("prepare failing host world", || {
        harness.create_host(
            &broken_name,
            concat!(
                "#cloud-config\n",
                "runcmd:\n",
                "  - |\n",
                "    echo attempt >> /var/lib/wt-host-attempts\n",
                "    echo 'broken host stdout'\n",
                "    echo 'broken host stderr' >&2\n",
                "    exit 42\n",
            ),
        )
    });
    assert_eq!(broken.status, InstanceStatus::Setup);
    harness.sync_inventory();
    let mut broken_setup = spawn_host_byobu(&harness, &broken_name);
    let failed = timings.run("retain failed host cloud-init", || {
        wait_for_host_status(
            &harness,
            &broken_name,
            &mut broken_setup,
            InstanceStatus::Error,
        )
    });
    let progress = capture_host_pane(&harness, &broken_name);
    let stdout = progress.find("broken host stdout").unwrap();
    let stderr = progress.find("broken host stderr").unwrap();
    assert!(
        stdout < stderr,
        "cloud-init output was not preserved in order:\n{progress}"
    );
    assert_eq!(
        count_disks(&harness.config.libvirt.worlds_dir),
        disks_before + 1
    );
    assert_eq!(failed.status, InstanceStatus::Error);
    insta::assert_snapshot!(
        failed.last_error.as_deref().unwrap(),
        @"guest reconciliation: host cloud-init failed: cloud-init final stage failed with exit status 1"
    );
    harness.sync_inventory();
    run_host(
        &harness,
        &broken_name,
        "test \"$(sudo -n cat /var/lib/wt-host-attempts | wc -l)\" = 1",
        "verify failed host setup ran once",
    );
    let _ = broken_setup.kill();
    let _ = broken_setup.wait();
    harness.delete(&broken_name);
    assert_eq!(
        count_disks(&harness.config.libvirt.worlds_dir),
        disks_before
    );

    let interrupted_name = unique_name("host-interrupted");
    let interrupted = harness.create_host(
        &interrupted_name,
        concat!(
            "#cloud-config\n",
            "runcmd:\n",
            "  - |\n",
            "    echo attempt >> /var/lib/wt-host-attempts\n",
            "    echo 'interrupt host setup now'\n",
            "    sleep 300\n",
        ),
    );
    assert_eq!(interrupted.status, InstanceStatus::Setup);
    harness.sync_inventory();
    let mut interrupted_setup = spawn_host_byobu(&harness, &interrupted_name);
    wait_for_live_host_output(
        &harness,
        &interrupted_name,
        &mut interrupted_setup,
        "interrupt host setup now",
    );
    run_host(
        &harness,
        &interrupted_name,
        "sudo systemctl kill --kill-whom=main --signal=KILL wt-host-setup.service",
        "interrupt host setup service",
    );
    let interrupted_error = wait_for_host_status(
        &harness,
        &interrupted_name,
        &mut interrupted_setup,
        InstanceStatus::Error,
    );
    insta::assert_snapshot!(
        interrupted_error.last_error.as_deref().unwrap(),
        @"guest reconciliation: host cloud-init failed: host cloud-init was interrupted"
    );
    run_host(
        &harness,
        &interrupted_name,
        "test \"$(sudo -n cat /var/lib/wt-host-attempts | wc -l)\" = 1",
        "verify interrupted host setup ran once",
    );
    let _ = interrupted_setup.kill();
    let _ = interrupted_setup.wait();
    harness.delete(&interrupted_name);
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
