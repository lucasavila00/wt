use super::support::{create, service, Worker};
use tempfile::TempDir;
use uuid::Uuid;
use wt_control_protocol::{Operation, Response, WindowId};
use wt_workload_registry::{NewWindow, Store};

#[test]
fn world_mail_is_listed_and_counted_only_for_the_world_owner() {
    let temp = TempDir::new().unwrap();
    let Response::World { world } = service(&temp, Worker::default())
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    let store = Store::open(&temp.path().join("worlds.db")).unwrap();
    let window_id = WindowId::new();
    store
        .insert_window(&NewWindow {
            window_id,
            world_id: world.world_id,
            owner: "tester".into(),
            tmux_window_id: Some("@1".into()),
            control_token: "token".into(),
            control_token_hash: "hash".into(),
            argv: vec!["codex".into()],
            cwd: "/home/wt".into(),
        })
        .unwrap();
    store
        .insert_world_mail(
            world.world_id,
            window_id,
            Uuid::new_v4(),
            "job is ready for review",
        )
        .unwrap();

    let Response::Worlds {
        world_mail_counts, ..
    } = service(&temp, Worker::default())
        .execute("tester", Operation::ListWorlds)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(world_mail_counts[&world.world_id], 1);

    let Response::WorldMail {
        messages,
        high_water_id,
    } = service(&temp, Worker::default())
        .execute(
            "tester",
            Operation::ListWorldMail {
                world_id: world.world_id,
                after_id: 0,
                limit: 10,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].world_id, world.world_id);
    assert_eq!(messages[0].window_id, window_id);
    assert_eq!(messages[0].message, "job is ready for review");
    assert_eq!(high_water_id, messages[0].id);

    let error = service(&temp, Worker::default())
        .execute(
            "someone-else",
            Operation::ListWorldMail {
                world_id: world.world_id,
                after_id: 0,
                limit: 10,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, wt_control_protocol::ErrorCode::NotFound);
}
