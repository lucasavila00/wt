use super::refresh::WorldSnapshot;
use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::sync::mpsc;

#[test]
fn world_view_reserves_the_top_row() {
    assert_eq!(world_rows(24), 23);
    assert_eq!(world_rows(1), 1);
    assert_eq!(world_area(Rect::new(0, 0, 80, 24)), Rect::new(0, 1, 80, 23));
}

#[test]
fn live_activity_uses_the_compact_terminal_viewport() {
    let area = Rect::new(0, 0, 100, 30);
    let mut model = ShellModel::new(vec!["local.one".into()]);
    assert_eq!(session_viewport(&model, area), (29, 100));

    model.handle_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ),
        area,
    );
    model.handle_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ),
        area,
    );

    assert_eq!(session_viewport(&model, area), (10, 91));
    assert_eq!(session_viewport(&model, Rect::new(0, 0, 400, 40)), (6, 95));
}

#[test]
fn mouse_input_skips_the_bar_and_is_translated_to_world_rows() {
    let area = Rect::new(0, 0, 80, 24);

    assert_eq!(world_mouse(mouse(4, 0), area), None);
    assert_eq!(world_mouse(mouse(4, 1), area).unwrap().row, 0);
    assert_eq!(world_mouse(mouse(4, 23), area).unwrap().row, 22);
}

fn mouse(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn local_mutation_invalidates_an_older_refresh() {
    let (sender, updates) = mpsc::sync_channel(1);
    sender
        .send(WorldSnapshot {
            generation: 4,
            instances: Vec::new(),
            failures: Vec::new(),
        })
        .unwrap();

    assert!(take_current_snapshot(&updates, 5).is_none());
}
