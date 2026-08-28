use super::*;
use wt_control_protocol::{PaneCell, PaneColor, PaneFrame};

fn gateway(temp: &tempfile::TempDir) -> Gateway {
    Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path: temp.path().join("instances.db"),
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap()
}

fn persisted_state(temp: &tempfile::TempDir) -> State {
    serde_json::from_slice(&std::fs::read(temp.path().join("gateway.json")).unwrap()).unwrap()
}

fn fail_future_saves(temp: &tempfile::TempDir) {
    std::fs::create_dir(temp.path().join("gateway.json.new")).unwrap();
}

#[test]
fn world_prompt_does_not_require_a_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let grant = GrantRecord {
        token: "token".into(),
        world_id: "world".into(),
    };

    assert_eq!(
        gateway.serve_cli(&["world-prompt".into()], &grant).unwrap(),
        world_prompt()
    );
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
fn deleted_grant_cannot_restore_cleared_pane_observations() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_id = Uuid::new_v4();
    let grant = gateway.reserve_grant(world_id).unwrap();
    let authorized = gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: grant.token.clone(),
            operation: ClientOperation::PaneObservations { panes: Vec::new() },
        })
        .unwrap();

    gateway
        .deactivate_pane_observations(world_id.into())
        .unwrap();
    gateway.revoke_grant(world_id).unwrap();

    let error = gateway
        .store_pane_observations(&[], &authorized)
        .unwrap_err();
    assert_eq!(error.to_string(), "gateway grant is invalid or revoked");
    assert!(gateway
        .pane_observations(world_id.into())
        .unwrap()
        .is_empty());
    let observations = gateway.pane_observations.lock().unwrap();
    assert!(!observations.inactive_worlds.contains(&world_id.into()));
    assert!(!observations.generations.contains_key(&world_id.into()));
}

#[test]
fn revocation_removes_the_grant_instead_of_retaining_a_tombstone() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_id = Uuid::new_v4();
    let grant = gateway.reserve_grant(world_id).unwrap();

    gateway.revoke_grant(world_id).unwrap();
    gateway.revoke_grant(world_id).unwrap();

    assert!(gateway.state.lock().unwrap().grants.is_empty());
    let persisted: State =
        serde_json::from_slice(&std::fs::read(temp.path().join("gateway.json")).unwrap()).unwrap();
    assert!(persisted.grants.is_empty());
    assert_eq!(
        gateway
            .authorize(&TransportRequest {
                protocol_version: PROTOCOL_VERSION,
                token: grant.token,
                operation: ClientOperation::Cli { args: Vec::new() },
            })
            .err()
            .unwrap()
            .to_string(),
        "gateway grant is invalid or revoked"
    );
}

#[test]
fn startup_reconciliation_removes_grants_without_worlds() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let existing = Uuid::new_v4();
    let orphan = Uuid::new_v4();
    let existing_grant = gateway.reserve_grant(existing).unwrap();
    let orphan_grant = gateway.reserve_grant(orphan).unwrap();

    gateway.reconcile_grants([existing.into()]).unwrap();

    let state = gateway.state.lock().unwrap();
    assert_eq!(state.grants.len(), 1);
    assert_eq!(state.grants[0].world_id, existing.to_string());
    drop(state);
    assert!(gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: existing_grant.token,
            operation: ClientOperation::Cli { args: Vec::new() },
        })
        .is_ok());
    assert_eq!(
        gateway
            .authorize(&TransportRequest {
                protocol_version: PROTOCOL_VERSION,
                token: orphan_grant.token,
                operation: ClientOperation::Cli { args: Vec::new() },
            })
            .err()
            .unwrap()
            .to_string(),
        "gateway grant is invalid or revoked"
    );
}

#[test]
fn failed_reservation_save_does_not_publish_the_grant_to_memory() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let existing = Uuid::new_v4();
    gateway.reserve_grant(existing).unwrap();
    fail_future_saves(&temp);

    assert!(gateway.reserve_grant(Uuid::new_v4()).is_err());

    let state = gateway.state.lock().unwrap();
    assert_eq!(state.grants.len(), 1);
    assert_eq!(state.grants[0].world_id, existing.to_string());
    drop(state);
    assert_eq!(persisted_state(&temp).grants.len(), 1);
}

#[test]
fn failed_reconciliation_save_preserves_the_persisted_and_memory_grants() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let existing = Uuid::new_v4();
    let orphan = Uuid::new_v4();
    gateway.reserve_grant(existing).unwrap();
    gateway.reserve_grant(orphan).unwrap();
    fail_future_saves(&temp);

    assert!(gateway.reconcile_grants([existing.into()]).is_err());

    assert_eq!(gateway.state.lock().unwrap().grants.len(), 2);
    assert_eq!(persisted_state(&temp).grants.len(), 2);
}

#[test]
fn failed_revocation_save_preserves_the_grant_but_deactivates_panes() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_uuid = Uuid::new_v4();
    let world_id = world_uuid.into();
    let grant = gateway.reserve_grant(world_uuid).unwrap();
    {
        let mut observations = gateway.pane_observations.lock().unwrap();
        observations.snapshots.insert(world_id, Vec::new());
        observations.generations.insert(world_id, 7);
    }
    fail_future_saves(&temp);

    assert!(gateway.revoke_grant(world_uuid).is_err());

    assert_eq!(gateway.state.lock().unwrap().grants.len(), 1);
    assert_eq!(persisted_state(&temp).grants.len(), 1);
    assert!(gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: grant.token,
            operation: ClientOperation::PaneObservations { panes: Vec::new() },
        })
        .is_ok());
    let observations = gateway.pane_observations.lock().unwrap();
    assert!(!observations.snapshots.contains_key(&world_id));
    assert!(!observations.generations.contains_key(&world_id));
    assert!(observations.inactive_worlds.contains(&world_id));
}

#[test]
fn world_run_epochs_reject_stale_and_inactive_pane_reports() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_id = Uuid::new_v4();
    let grant = gateway.reserve_grant(world_id).unwrap();
    let authorized = gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: grant.token.clone(),
            operation: ClientOperation::PaneObservations { panes: Vec::new() },
        })
        .unwrap();

    assert!(
        gateway
            .control(ControlRequest::DeactivatePaneObservations {
                world_id: world_id.to_string(),
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
    let inactive = gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: grant.token.clone(),
            operation: ClientOperation::PaneObservations { panes: Vec::new() },
        })
        .unwrap();
    assert_eq!(
        gateway
            .store_pane_observations(&[], &inactive)
            .unwrap_err()
            .to_string(),
        "pane observations are inactive for this world"
    );

    assert!(
        gateway
            .control(ControlRequest::ActivatePaneObservations {
                world_id: world_id.to_string(),
            })
            .unwrap()
            .ok
    );
    assert_eq!(
        gateway
            .store_pane_observations(&[], &inactive)
            .unwrap_err()
            .to_string(),
        "pane observation belongs to an expired world run"
    );
    let current = gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: grant.token,
            operation: ClientOperation::PaneObservations { panes: Vec::new() },
        })
        .unwrap();
    gateway.store_pane_observations(&[], &current).unwrap();
}
