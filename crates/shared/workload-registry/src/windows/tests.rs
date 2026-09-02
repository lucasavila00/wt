use super::*;
use crate::{NewWorld, Resources};
use wt_control_protocol::{WorldName, WorldStatus};

fn fixture() -> (tempfile::TempDir, Store, WorldId, NewWindow) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("registry.db")).unwrap();
    let world_id = WorldId::new();
    store
        .insert_with_capacity_limit(
            &NewWorld {
                world_id,
                owner: "owner".into(),
                name: WorldName::parse("host").unwrap(),
                status: WorldStatus::Running,
                vcpus: 1,
                memory_mib: 1024,
                disk_gib: 8,
                setup_fingerprint: "test".into(),
            },
            Resources::UNLIMITED,
        )
        .unwrap();
    let window = NewWindow {
        window_id: WindowId::new(),
        world_id,
        owner: "owner".into(),
        tmux_window_id: Some("@7".into()),
        control_token: "token".into(),
        control_token_hash: "hash".into(),
        argv: vec!["sh".into(), "-c".into(), "echo hi".into()],
        cwd: "/home/wt".into(),
    };
    (temp, store, world_id, window)
}

#[test]
fn stores_window_and_resolves_native_tmux_identity() {
    let (_temp, store, world_id, window) = fixture();
    store.insert_window(&window).unwrap();
    assert_eq!(
        store.window_id_by_tmux(world_id, "@7").unwrap(),
        window.window_id
    );
    assert_eq!(
        store
            .get_owned_window("owner", window.window_id)
            .unwrap()
            .argv,
        window.argv
    );
    assert!(matches!(
        store.get_owned_window("other", window.window_id),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn output_is_ordered_and_input_is_committed_in_order() {
    let (_temp, store, _world_id, window) = fixture();
    store.insert_window(&window).unwrap();
    store
        .append_window_output(
            window.window_id,
            &[
                ("stdout".into(), b"one".to_vec()),
                ("stderr".into(), b"two".to_vec()),
            ],
        )
        .unwrap();
    let page = store.window_output(window.window_id, 0, 1).unwrap();
    assert_eq!((page.output[0].record_id, page.next_after), (1, 1));
    let first_request = uuid::Uuid::new_v4();
    assert_eq!(
        store
            .enqueue_window_input(window.window_id, first_request, b"a")
            .unwrap(),
        1
    );
    assert!(matches!(
        store.enqueue_window_input(window.window_id, first_request, b"different"),
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        store
            .enqueue_window_input(window.window_id, first_request, b"a")
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .enqueue_window_input(window.window_id, uuid::Uuid::new_v4(), b"b")
            .unwrap(),
        2
    );
    assert_eq!(
        store
            .pending_window_input(window.window_id)
            .unwrap()
            .iter()
            .map(|item| item.sequence_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    store.acknowledge_window_input(window.window_id, 1).unwrap();
    assert_eq!(
        store.pending_window_input(window.window_id).unwrap()[0].sequence_id,
        2
    );
    store.acknowledge_window_input(window.window_id, 2).unwrap();
    assert_eq!(
        store
            .enqueue_window_input(window.window_id, uuid::Uuid::new_v4(), b"c")
            .unwrap(),
        3
    );
}

#[test]
fn deleting_a_world_cascades_its_windows() {
    let (_temp, store, world_id, window) = fixture();
    store.insert_window(&window).unwrap();
    store.delete(world_id).unwrap();
    assert!(matches!(
        store.get_owned_window("owner", window.window_id),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn output_retention_removes_complete_records_and_reports_a_gap() {
    let (_temp, store, _world_id, window) = fixture();
    store.insert_window(&window).unwrap();
    store
        .append_window_output(
            window.window_id,
            &[
                (
                    "stdout".into(),
                    vec![0; WINDOW_OUTPUT_RETENTION_BYTES as usize],
                ),
                ("stderr".into(), vec![1]),
            ],
        )
        .unwrap();
    let page = store.window_output(window.window_id, 0, 10).unwrap();
    assert!(page.gap);
    assert_eq!(
        (
            page.oldest_available,
            page.output.len(),
            page.output[0].record_id
        ),
        (2, 1, 2)
    );
}
