use super::*;
use crate::shell::control::{CodexCard, CodexCardIdentity};
use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState};

#[test]
fn completed_focus_switches_only_from_the_active_codex_view() {
    let mut model = model_with_open_card();
    open_codex_activity(&mut model);
    let InputRoute::OpenCodex(target) = model.handle_key(key(KeyCode::Enter), area()) else {
        panic!("live card did not produce an open target");
    };
    model.finish_codex_open(&target, Some(1), false);
    assert_eq!(model.active(), 1);
    assert_eq!(model.mode(), Mode::World);

    let mut canceled = model_with_open_card();
    open_codex_activity(&mut canceled);
    let InputRoute::OpenCodex(target) = canceled.handle_key(key(KeyCode::Enter), area()) else {
        panic!("live card did not produce an open target");
    };
    canceled.handle_key(key(KeyCode::F(5)), area());
    canceled.finish_codex_open(&target, Some(1), false);
    assert_eq!(canceled.active(), 0);
    assert_eq!(canceled.mode(), Mode::World);
}

fn model_with_open_card() -> ShellModel {
    let mut model = ShellModel::new(vec![
        ShellWorld::test("local.one", 1),
        ShellWorld::test("local.two", 2),
        ShellWorld::test("local.three", 3),
    ]);
    model.handle_key(key(KeyCode::F(5)), area());
    let session_id = Uuid::from_u128(10);
    let world_id = Uuid::from_u128(2);
    let target = ByobuTarget {
        tmux_session: "wt-host".into(),
        pane_id: "%1".into(),
    };
    model.set_codex(
        vec![CodexCard {
            identity: CodexCardIdentity::Observation {
                context: "local".into(),
                session_id,
                world_id,
                tmux_session: target.tmux_session.clone(),
                pane_id: target.pane_id.clone(),
            },
            context: "local".into(),
            session_id: Some(session_id),
            timestamp: Some(1),
            latest_user_message: Some("Focus this latest request".into()),
            kind: super::super::control::CodexCardKind::Observation {
                world_id,
                world_name: "two".into(),
                cwd: "/home/wt/project".into(),
                repository_root: None,
                repository_url: None,
                git_branch: None,
                state: CodexSessionState::Working,
                session_start_source: None,
                target,
            },
        }],
        "2026-08-21T20:00:00Z".into(),
        area(),
    );
    model
}

fn open_codex_activity(model: &mut ShellModel) {
    for code in [KeyCode::F(5), KeyCode::Up] {
        model.handle_key(key(code), area());
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn area() -> Rect {
    Rect::new(0, 0, 80, 24)
}
