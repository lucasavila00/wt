use super::*;
use wt_control_protocol::{PaneCell, PaneColor, PaneFrame};

fn gateway(temp: &tempfile::TempDir) -> Gateway {
    Gateway::open(
        GatewayConfig {
            providers: vec![Provider::Local {
                host: "github.com".into(),
                repositories: temp.path().to_owned(),
                api: None,
            }],
        },
        ActivityRecorder::open(&temp.path().join("instances.db")).unwrap(),
    )
    .unwrap()
}

#[test]
fn world_prompt_does_not_require_a_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);

    assert_eq!(
        gateway
            .serve_cli(&["world-prompt".into()], Uuid::nil().into())
            .unwrap(),
        world_prompt()
    );
}

#[test]
fn parent_messages_derive_the_world_and_enforce_the_message_limit() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_id = Uuid::new_v4().into();
    let request = |message: String| TransportRequest {
        protocol_version: PROTOCOL_VERSION,
        operation: ClientOperation::SendMessageToParent { message },
    };

    assert_eq!(
        gateway
            .authorize(&request("done".into()), world_id)
            .unwrap()
            .world_id,
        world_id
    );
    for message in [
        String::new(),
        "x".repeat(wt_workload_registry::MAX_MAIL_MESSAGE_BYTES + 1),
    ] {
        assert_eq!(
            gateway
                .authorize(&request(message), world_id)
                .unwrap_err()
                .to_string(),
            "message must contain 1 to 67108864 UTF-8 bytes"
        );
    }
}

#[test]
fn parent_message_transport_commits_world_scoped_mail() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("instances.db");
    let store = wt_workload_registry::Store::open(&path).unwrap();
    let world_id = Uuid::new_v4().into();
    store
        .insert(&wt_workload_registry::NewWorld {
            world_id,
            owner: "owner".into(),
            name: wt_control_protocol::WorldName::parse("world").unwrap(),
            status: wt_control_protocol::WorldStatus::Running,
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 10,
            setup_fingerprint: "fingerprint".into(),
        })
        .unwrap();
    drop(store);
    let gateway = gateway(&temp);
    let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    crate::write_json_line(
        &mut client,
        &TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::SendMessageToParent {
                message: "ready".into(),
            },
        },
    )
    .unwrap();

    gateway.handle_transport(server, world_id).unwrap();

    let response: TransportResponse = crate::read_json_line(&mut client).unwrap();
    assert!(response.ok);
    let page = wt_workload_registry::Store::open(&path)
        .unwrap()
        .list_world_mail("owner", world_id, 0, 10)
        .unwrap();
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].world_id, world_id);
    assert_eq!(page.messages[0].message, "ready");
}

#[test]
fn pane_observations_are_complete_transient_world_snapshots() {
    let frame = PaneFrame {
        rows: 1,
        columns: 1,
        cells: vec![PaneCell {
            text: "C".into(),
            foreground: PaneColor::Default,
            background: PaneColor::Default,
            bold: false,
            italic: false,
            underlined: false,
            inverse: false,
        }],
    };
    let panes = [crate::PaneObservation {
        tmux_session: "wt-host".into(),
        pane_id: "%1".into(),
        window_index: 0,
        window_name: "codex".into(),
        screen_fingerprint: "a".repeat(64),
        cwd: "/home/wt".into(),
        git_branch: None,
        frame: frame.clone(),
    }];
    let world_id = Uuid::nil().into();
    let mut observations = std::collections::BTreeMap::new();

    replace_pane_observations(&mut observations, world_id, &panes, 10);
    assert_eq!(observations[&world_id][0].render.frame, frame);
    assert_eq!(observations[&world_id][0].changed_at_unix_ms, 10);

    let mut renamed = panes[0].clone();
    renamed.window_index = 1;
    renamed.window_name = "make".into();
    renamed.cwd = "/home/wt/wt".into();
    replace_pane_observations(&mut observations, world_id, &[renamed.clone()], 20);
    assert_eq!(observations[&world_id][0].changed_at_unix_ms, 10);
    assert_eq!(observations[&world_id][0].observed_at_unix_ms, 20);
    assert_eq!(observations[&world_id][0].render.window_index, 1);
    assert_eq!(observations[&world_id][0].render.window_name, "make");
    assert_eq!(observations[&world_id][0].cwd, "/home/wt/wt");

    renamed.screen_fingerprint = "b".repeat(64);
    replace_pane_observations(&mut observations, world_id, &[renamed], 30);
    assert_eq!(observations[&world_id][0].changed_at_unix_ms, 30);

    replace_pane_observations(&mut observations, world_id, &[], 40);
    assert!(!observations.contains_key(&world_id));
}

#[test]
fn world_run_epochs_reject_stale_and_inactive_pane_reports() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_uuid = Uuid::new_v4();
    let world_id = world_uuid.into();
    let request = TransportRequest {
        protocol_version: PROTOCOL_VERSION,
        operation: ClientOperation::PaneObservations { panes: Vec::new() },
    };
    let authorized = gateway.authorize(&request, world_id).unwrap();

    assert!(
        gateway
            .control(ControlRequest::DeactivateWorld {
                world_id: world_uuid.to_string(),
            })
            .unwrap()
            .ok
    );
    assert_eq!(
        gateway
            .store_pane_observations(&[], &authorized)
            .unwrap_err()
            .to_string(),
        "pane observation belongs to an expired world run"
    );
    assert_eq!(
        gateway
            .authorize(&request, world_id)
            .unwrap_err()
            .to_string(),
        "agent tools are inactive for this world"
    );

    assert!(
        gateway
            .control(ControlRequest::ActivateWorld {
                world_id: world_uuid.to_string(),
            })
            .unwrap()
            .ok
    );
    assert_eq!(
        gateway
            .store_pane_observations(&[], &authorized)
            .unwrap_err()
            .to_string(),
        "pane observation belongs to an expired world run"
    );
    let current = gateway.authorize(&request, world_id).unwrap();
    gateway.store_pane_observations(&[], &current).unwrap();
}
