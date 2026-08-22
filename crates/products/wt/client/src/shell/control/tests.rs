use super::*;
use wt_control_protocol::ByobuTarget;

#[test]
fn tab_cycles_activities() {
    let mut state = ControlState::default();
    assert_eq!(state.activity(), Activity::Codex);
    state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area());
    assert_eq!(state.activity(), Activity::Worlds);
    state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area());
    assert_eq!(state.activity(), Activity::Codex);
}

#[test]
fn f1_and_one_open_the_command_palette() {
    for code in [KeyCode::F(1), KeyCode::Char('1')] {
        let mut state = ControlState::default();
        state.handle_key(KeyEvent::new(code, KeyModifiers::NONE), area());
        assert!(state.palette().is_open());
    }
}

#[test]
fn palette_filters_selects_and_returns_commands() {
    let mut state = ControlState::default();
    state.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE), area());
    for character in "new".chars() {
        state.handle_key(
            KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            area(),
        );
    }
    assert_eq!(state.palette().matches(), vec![ControlCommand::NewWorld]);
    assert_eq!(
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area()),
        Some(ControlAction::Command(ControlCommand::NewWorld))
    );
    assert!(!state.palette().is_open());
}

#[test]
fn activity_icons_and_palette_results_are_clickable() {
    let mut state = ControlState::default();
    let area = Rect::new(0, 0, 64, 16);
    assert!(state.handle_mouse(mouse(1, 4), area).0);
    assert_eq!(state.activity(), Activity::Worlds);
    state.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE), area);
    let (_, results) = command_palette_layout(control_areas(area).1);
    assert_eq!(
        state.handle_mouse(mouse(results.x, results.y), area),
        (true, Some(ControlAction::Command(ControlCommand::NewWorld)))
    );
    assert!(!state.palette().is_open());
    state.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE), area);
    assert!(state.handle_mouse(mouse(results.x, results.y + 3), area).0);
    assert!(state.palette().is_open());
}

#[test]
fn card_navigation_opens_only_the_selected_live_location() {
    let mut state = ControlState::default();
    let first = live_card(1, "%1");
    state.set_codex(
        vec![
            first.clone(),
            CodexCard::rollout_only("ars", Uuid::from_u128(2), 2, None, None),
        ],
        "2026-08-21T20:00:00Z".into(),
        area(),
    );
    state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), area());
    assert!(state
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area())
        .is_none());
    assert_eq!(state.selected(), Some(&state.codex()[1].identity));
    state.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), area());
    let Some(ControlAction::OpenCodex(target)) =
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area())
    else {
        panic!("live card did not produce an open target");
    };
    assert_eq!(target.identity, first.identity);
    assert_eq!(target.pane_id, "%1");
    assert!(state
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area())
        .is_none());
}

#[test]
fn card_clicks_use_rendered_rectangles_and_wheel_moves_selection() {
    let mut state = ControlState::default();
    state.set_codex(
        vec![live_card(1, "%1"), live_card(2, "%2")],
        "2026-08-21T20:00:00Z".into(),
        area(),
    );
    let second = codex_card_rects(area(), 0, 2)[1].1;
    let (changed, action) = state.handle_mouse(mouse(second.x + 1, second.y + 1), area());
    assert!(changed);
    let Some(ControlAction::OpenCodex(target)) = action else {
        panic!("live card click did not produce an open target");
    };
    assert_eq!(target.pane_id, "%2");
    state.finish_open(&target, true);
    let scroll = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: second.x,
        row: second.y,
        modifiers: KeyModifiers::NONE,
    };
    assert!(state.handle_mouse(scroll, area()).0);
    assert_eq!(state.selected(), Some(&state.codex()[0].identity));
}

#[test]
fn snapshot_times_track_the_last_applied_snapshot() {
    let mut state = ControlState::default();

    state.set_worlds_updated_at("2026-08-21T20:00:00Z".into());
    state.set_worlds_updated_at("2026-08-21T20:00:05Z".into());
    state.set_codex(Vec::new(), "2026-08-21T20:00:01Z".into(), area());
    state.set_codex(Vec::new(), "2026-08-21T20:00:06Z".into(), area());

    assert_eq!(state.worlds_updated_at(), Some("2026-08-21T20:00:05Z"));
    assert_eq!(state.codex_updated_at(), Some("2026-08-21T20:00:06Z"));
}

#[test]
fn refresh_and_navigation_do_not_hide_an_opening_card() {
    let mut state = ControlState::default();
    state.set_codex(
        vec![live_card(1, "%1"), live_card(2, "%2")],
        "2026-08-21T20:00:00Z".into(),
        area(),
    );
    let Some(ControlAction::OpenCodex(target)) =
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area())
    else {
        panic!("live card did not produce an open target");
    };

    assert!(!state.set_codex(Vec::new(), "2026-08-21T20:00:05Z".into(), area()));
    state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), area());
    assert_eq!(state.selected(), Some(&target.identity));
    assert_eq!(state.codex_updated_at(), Some("2026-08-21T20:00:00Z"));

    assert!(state.finish_open(&target, true));
    assert!(state.open_failed());

    let Some(ControlAction::OpenCodex(retry)) =
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area())
    else {
        panic!("failed open did not produce a retry target");
    };
    assert_eq!(retry.identity, target.identity);
    assert!(!state.open_failed());

    assert!(state.finish_open(&retry, true));
    let (retry_area, _) = super::super::toast::actions(area());
    let (_, action) = state.handle_mouse(mouse(retry_area.x, retry_area.y), area());
    let Some(ControlAction::OpenCodex(clicked_retry)) = action else {
        panic!("clicking Retry did not produce an open target");
    };
    assert_eq!(clicked_retry.identity, target.identity);

    assert!(state.finish_open(&clicked_retry, true));
    let (_, dismiss) = super::super::toast::actions(area());
    let (changed, action) = state.handle_mouse(mouse(dismiss.x, dismiss.y), area());
    assert!(changed);
    assert!(action.is_none());
    assert!(!state.open_failed());
}

#[test]
fn context_failure_retry_and_dismiss_are_explicit() {
    let mut state = ControlState::default();
    state.set_context_failures(vec!["ars".into()]);
    assert_eq!(state.context_failure().unwrap(), ["ars"]);
    assert_eq!(
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area()),
        Some(ControlAction::RefreshCodex)
    );

    state.set_context_failures(vec!["ars".into()]);
    state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), area());
    assert!(state.context_failure().is_none());
    state.set_context_failures(vec!["ars".into()]);
    assert!(state.context_failure().is_none());

    state.set_context_failures(Vec::new());
    state.set_context_failures(vec!["ars".into()]);
    let (retry, _) = super::super::toast::actions(area());
    assert_eq!(
        state.handle_mouse(mouse(retry.x, retry.y), area()),
        (true, Some(ControlAction::RefreshCodex))
    );
}

#[test]
fn refresh_keeps_the_selected_card_in_its_viewport() {
    let mut state = ControlState::default();
    let cards = (1..=6)
        .map(|index| live_card(index, &format!("%{index}")))
        .collect::<Vec<_>>();
    state.set_codex(cards.clone(), "2026-08-21T20:00:00Z".into(), area());
    for _ in 0..5 {
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), area());
    }
    let selected = state.selected().cloned();
    assert_eq!(state.codex_offset(), 3);

    assert!(state.set_codex(cards, "2026-08-21T20:00:05Z".into(), area()));
    assert_eq!(state.selected(), selected.as_ref());
    assert_eq!(state.codex_offset(), 3);
}

#[test]
fn resize_keeps_the_selected_card_in_its_viewport() {
    let mut state = ControlState::default();
    let tall = Rect::new(0, 0, 64, 40);
    state.set_codex(
        (1..=6)
            .map(|index| live_card(index, &format!("%{index}")))
            .collect(),
        "2026-08-21T20:00:00Z".into(),
        tall,
    );
    for _ in 0..5 {
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), tall);
    }
    assert_eq!(state.codex_offset(), 0);

    state.resize(Rect::new(0, 0, 64, 10));
    assert_eq!(state.codex_offset(), 4);
}

fn live_card(index: u128, pane_id: &str) -> CodexCard {
    let session_id = Uuid::from_u128(index);
    let world_id = Uuid::from_u128(100 + index);
    let identity = CodexCardIdentity::Observation {
        context: "ars".into(),
        session_id,
        world_id,
        tmux_session: "wt-host".into(),
        pane_id: pane_id.into(),
    };
    CodexCard {
        identity,
        context: "ars".into(),
        session_id: Some(session_id),
        timestamp: Some(index as i64),
        title: Some(format!("Session {index}")),
        latest_user_message: Some(format!("Latest message {index}")),
        kind: CodexCardKind::Observation {
            world_id,
            world_name: "dev".into(),
            cwd: "/home/wt/project".into(),
            repository_root: None,
            repository_url: None,
            git_branch: None,
            state: CodexSessionState::Working,
            session_start_source: None,
            target: ByobuTarget {
                tmux_session: "wt-host".into(),
                pane_id: pane_id.into(),
            },
        },
    }
}

fn mouse(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn area() -> Rect {
    Rect::new(0, 0, 64, 20)
}
