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

#[test]
fn world_prompt_does_not_require_a_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let grant = GrantRecord {
        id: "id".into(),
        token: "token".into(),
        world_id: "world".into(),
        revoked: false,
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
fn revoked_grant_cannot_restore_cleared_pane_observations() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = gateway(&temp);
    let world_id = Uuid::new_v4();
    let grant = gateway.reserve_grant(world_id).unwrap();
    let authorized = gateway
        .authorize(&TransportRequest {
            protocol_version: PROTOCOL_VERSION,
            token: grant.token,
            operation: ClientOperation::PaneObservations { panes: Vec::new() },
        })
        .unwrap();

    gateway.revoke_grant(&grant.id).unwrap();

    let error = gateway
        .store_pane_observations(&[], &authorized)
        .unwrap_err();
    assert_eq!(error.to_string(), "gateway grant is invalid or revoked");
    assert!(gateway
        .pane_observations(world_id.into())
        .unwrap()
        .is_empty());
}
