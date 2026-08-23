use super::*;
use std::time::{Duration, Instant};
use wt_control_protocol::{Operation, Response};

#[test]
#[ignore = "requires installed KVM image and host integration"]
fn branch_changes_during_a_working_turn_without_another_codex_hook() {
    let _lock = acquire_kvm_test_lock();
    let mut timings = Timings::new();
    let name = unique_name("git-tracker");
    let harness = KvmHarness::new(&mut timings);
    let created = timings.run("create Git tracking world", || harness.create(&name));
    assert_eq!(created.status, InstanceStatus::Running);

    let session_id = "123e4567-e89b-12d3-a456-426614174000";
    run_guest(
        &harness,
        &name,
        &format!(
            "set -eu; git clone https://local.test/acme/widget.git /home/wt/project; cd /home/wt/project; git switch -c wt/tracker-before; tmux new-window -d -t wt-host -n git-tracker 'cd /home/wt/project; exec bash'; pane=$(tmux list-panes -t wt-host:git-tracker -F '#{{pane_id}}'); tmux send-keys -t \"$pane\" \"printf '%s\\n' '{{\\\"session_id\\\":\\\"{session_id}\\\",\\\"cwd\\\":\\\"/home/wt/project\\\",\\\"hook_event_name\\\":\\\"UserPromptSubmit\\\"}}' | WT_BYOBU_SESSION=wt-host WT_BYOBU_PANE=$pane wt-codex-integration report-hook\" C-m; for attempt in $(seq 1 50); do test -f /home/wt/.local/state/wt/codex-git-tracker.json && break; sleep 0.1; done; test -f /home/wt/.local/state/wt/codex-git-tracker.json; git switch -c wt/tracker-after",
        ),
        "start a working Codex session and switch its branch",
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let Response::CodexSessions { sessions } = call_api(
            harness.temp.path(),
            &harness.server_config_path,
            Operation::ListCodexSessions,
        ) else {
            panic!("expected Codex session response");
        };
        let observation = sessions
            .iter()
            .find(|session| session.session_id.to_string() == session_id)
            .and_then(|session| session.observations.first());
        if observation.is_some_and(|observation| {
            observation.git_branch.as_deref() == Some("wt/tracker-after")
                && observation.repository_url.as_deref()
                    == Some("https://local.test/acme/widget.git")
                && observation.state == wt_control_protocol::CodexSessionState::Working
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Git tracker did not report branch change"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
