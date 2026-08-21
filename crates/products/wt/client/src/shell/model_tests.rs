use super::*;
use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState};

fn model() -> ShellModel {
    let mut model = ShellModel::new(vec![world("one"), world("two"), world("three")]);
    model.handle_key(key(KeyCode::F(5)), area());
    model
}

fn world(name: &str) -> ShellWorld {
    ShellWorld::test(name, Uuid::new_v4().as_u128())
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn shifted(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

#[test]
fn world_mode_forwards_every_key_except_f5() {
    let mut model = model();

    assert_eq!(
        model.handle_key(key(KeyCode::Left), area()),
        InputRoute::World
    );
    assert_eq!(model.active(), 0);
    assert_eq!(
        model.handle_key(key(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert_eq!(model.mode(), Mode::Switcher);
}

#[test]
fn shift_f5_disables_the_override_and_plain_f5_reaches_the_world() {
    let mut model = model();

    assert_eq!(
        model.handle_key(shifted(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert!(model.f5_disabled());
    assert_eq!(model.mode(), Mode::World);
    assert_eq!(
        model.handle_key(key(KeyCode::F(5)), area()),
        InputRoute::World
    );

    assert_eq!(
        model.handle_key(shifted(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert!(!model.f5_disabled());
    assert_eq!(
        model.handle_key(key(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert_eq!(model.mode(), Mode::Switcher);
}

#[test]
fn shell_starts_in_control_mode() {
    let model = ShellModel::new(vec![world("one")]);

    assert_eq!(model.mode(), Mode::Control);
}

#[test]
fn world_cards_select_and_open_worlds() {
    let mut model = ShellModel::new(vec![world("one"), world("two"), world("three")]);
    model.handle_key(key(KeyCode::Tab), area());

    model.handle_key(key(KeyCode::Down), area());
    assert_eq!(model.active_world(), "two");
    assert_eq!(model.mode(), Mode::Control);

    model.handle_key(key(KeyCode::Enter), area());
    assert_eq!(model.active_world(), "two");
    assert_eq!(model.mode(), Mode::World);
}

#[test]
fn empty_shell_stays_in_control_mode_on_f5() {
    let mut model = ShellModel::new(Vec::new());

    assert_eq!(
        model.handle_key(key(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert_eq!(
        model.handle_key(shifted(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert!(!model.f5_disabled());
    assert_eq!(model.mode(), Mode::Control);
}

#[test]
fn switcher_cycles_worlds_without_leaving_the_bar() {
    let mut model = model();
    model.handle_key(key(KeyCode::F(5)), area());

    assert_eq!(
        model.handle_key(key(KeyCode::Left), area()),
        InputRoute::Consumed
    );
    assert_eq!(model.active(), 2);
    assert_eq!(
        model.handle_key(key(KeyCode::Right), area()),
        InputRoute::Consumed
    );
    assert_eq!(
        model.handle_key(key(KeyCode::Right), area()),
        InputRoute::Consumed
    );
    assert_eq!(model.active(), 1);
    assert_eq!(model.mode(), Mode::Switcher);
}

#[test]
fn switcher_forwards_unadvertised_keys_to_the_world() {
    let mut model = model();
    model.handle_key(key(KeyCode::F(5)), area());

    assert_eq!(
        model.handle_key(key(KeyCode::Char('x')), area()),
        InputRoute::World
    );
    assert_eq!(model.mode(), Mode::Switcher);
}

#[test]
fn up_opens_control_and_f5_closes_it() {
    let mut model = model();
    model.handle_key(key(KeyCode::F(5)), area());
    assert_eq!(
        model.handle_key(key(KeyCode::Up), area()),
        InputRoute::Consumed
    );

    assert_eq!(model.mode(), Mode::Control);
    assert_eq!(
        model.handle_key(key(KeyCode::Left), area()),
        InputRoute::Consumed
    );
    assert_eq!(model.active(), 0);
    model.handle_key(key(KeyCode::F(1)), area());
    assert!(model.control().palette().is_open());
    assert_eq!(
        model.handle_key(key(KeyCode::F(5)), area()),
        InputRoute::Consumed
    );
    assert_eq!(model.mode(), Mode::World);
    assert!(!model.control().palette().is_open());
}

#[test]
fn f5_closes_the_switcher() {
    let mut model = model();
    model.handle_key(key(KeyCode::F(5)), area());
    model.handle_key(key(KeyCode::F(5)), area());

    assert_eq!(model.mode(), Mode::World);
}

#[test]
fn switcher_forwards_mouse_to_the_world() {
    assert!(Mode::World.forwards_mouse());
    assert!(Mode::Switcher.forwards_mouse());
    assert!(!Mode::Control.forwards_mouse());
}

#[test]
fn clicking_the_world_bar_activates_it_and_clicking_arrows_changes_worlds() {
    let mut model = model();

    assert!(model.handle_mouse(mouse(0, 0), area()).0);
    assert_eq!(model.mode(), Mode::Switcher);
    let [previous, _, _] = model.world_bar_controls(area());
    model.handle_mouse(mouse(previous.x, previous.y), area());
    assert_eq!(model.active(), 2);
    let [_, _, next] = model.world_bar_controls(area());
    model.handle_mouse(mouse(next.x, next.y), area());
    assert_eq!(model.active(), 0);
}

#[test]
fn clicking_a_disabled_world_bar_restores_the_override() {
    let mut model = model();
    model.handle_key(shifted(KeyCode::F(5)), area());

    assert!(model.f5_disabled());
    assert!(model.handle_mouse(mouse(0, 0), area()).0);
    assert!(!model.f5_disabled());
    assert_eq!(model.mode(), Mode::Switcher);
}

#[test]
fn f6_closes_from_every_mode_without_forwarding() {
    for mode in [Mode::World, Mode::Switcher, Mode::Control] {
        let mut model = model();
        model.mode = mode;

        assert_eq!(
            model.handle_key(key(KeyCode::F(6)), area()),
            InputRoute::Consumed
        );
        assert!(model.should_quit());
    }
}

#[test]
fn completed_focus_switches_only_from_the_active_codex_view() {
    let mut model = model_with_open_card();
    open_codex_activity(&mut model);
    let InputRoute::OpenCodex(target) = model.handle_key(key(KeyCode::Enter), area()) else {
        panic!("live card did not produce an open target");
    };
    model.finish_codex_open(&target.identity, Some(1), None);
    assert_eq!(model.active(), 1);
    assert_eq!(model.mode(), Mode::World);

    let mut canceled = model_with_open_card();
    open_codex_activity(&mut canceled);
    let InputRoute::OpenCodex(target) = canceled.handle_key(key(KeyCode::Enter), area()) else {
        panic!("live card did not produce an open target");
    };
    canceled.handle_key(key(KeyCode::F(5)), area());
    canceled.finish_codex_open(&target.identity, Some(1), None);
    assert_eq!(canceled.active(), 0);
    assert_eq!(canceled.mode(), Mode::World);
}

fn model_with_open_card() -> ShellModel {
    let mut model = model();
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
            kind: super::super::control::CodexCardKind::Observation {
                world_id,
                world_name: "two".into(),
                cwd: "/home/wt/project".into(),
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

#[test]
fn reconciliation_preserves_the_active_world_or_selects_the_first() {
    let mut model = model();
    model.active = 1;

    let active = model.worlds[1].clone();
    model.reconcile_worlds(vec![world("zero"), active, world("four")]);
    assert_eq!(model.active_world(), "two");

    model.reconcile_worlds(vec![world("four"), world("zero")]);
    assert_eq!(model.active_world(), "four");
}

#[test]
fn reconciliation_opens_control_when_all_worlds_are_removed() {
    let mut model = model();

    model.reconcile_worlds(Vec::new());

    assert!(!model.has_worlds());
    assert!(!model.f5_disabled());
    assert_eq!(model.mode(), Mode::Control);
}

#[test]
fn world_identity_includes_the_context() {
    let id = Uuid::new_v4();
    let local = ShellWorld::test("local.same", id.as_u128());
    let lab = ShellWorld::test("lab.same", id.as_u128());
    let model = ShellModel::new(vec![local.clone(), lab.clone()]);

    assert_eq!(model.world_index(&local.identity), Some(0));
    assert_eq!(model.world_index(&lab.identity), Some(1));
}

fn area() -> Rect {
    Rect::new(0, 0, 80, 24)
}

fn mouse(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}
