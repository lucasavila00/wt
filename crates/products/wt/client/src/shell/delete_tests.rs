use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use uuid::Uuid;
use wt_control_protocol::InstanceName;

fn world(name: &str) -> ShellWorld {
    let (context, instance) = name.split_once('.').unwrap();
    ShellWorld {
        identity: WorldIdentity {
            context: context.into(),
            id: Uuid::new_v4(),
        },
        name: name.into(),
        instance_name: InstanceName::parse(instance).unwrap(),
        control_alias: format!("{name}-direct"),
        status: wt_control_protocol::InstanceStatus::Running,
        resources: "2 CPU · 4G · 1G/32G disk".into(),
        detail: "-".into(),
    }
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn fuzzy_search_suggests_the_matching_world() {
    let mut picker = Picker::new(vec![world("local.alpha"), world("lab.topic-world")]);
    for character in "tpw".chars() {
        let _ = picker.handle_event(&key(KeyCode::Char(character)), Rect::new(0, 0, 80, 24));
    }
    assert_eq!(picker.matches, vec![1]);
}

#[test]
fn mouse_wheel_scrolls_results_and_click_uses_the_visible_row() {
    let worlds = (0..20)
        .map(|index| world(&format!("local.world-{index}")))
        .collect::<Vec<_>>();
    let mut picker = Picker::new(worlds);
    let area = Rect::new(0, 0, 40, 10);
    let (_, results) = picker_layout(area);
    let mouse = |kind| {
        Event::Mouse(MouseEvent {
            kind,
            column: results.x,
            row: results.y,
            modifiers: KeyModifiers::NONE,
        })
    };
    let _ = picker.handle_event(&mouse(MouseEventKind::ScrollDown), area);
    assert_eq!(picker.offset, 1);
    assert!(matches!(
        picker.handle_event(&mouse(MouseEventKind::Down(MouseButton::Left)), area),
        PickerEvent::Select(ShellWorld { ref name, .. }) if name == "local.world-1"
    ));
}

#[test]
fn confirmation_starts_on_cancel_and_requires_right_then_enter() {
    let area = Rect::new(0, 0, 100, 30);
    let mut choice = ConfirmChoice::Cancel;
    assert!(matches!(
        confirmation_event(&key(KeyCode::Enter), area, &mut choice),
        ConfirmationEvent::Cancel
    ));
    assert!(matches!(
        confirmation_event(&key(KeyCode::Right), area, &mut choice),
        ConfirmationEvent::Changed
    ));
    assert!(matches!(
        confirmation_event(&key(KeyCode::Enter), area, &mut choice),
        ConfirmationEvent::Delete
    ));
}

#[test]
fn delete_button_is_directly_clickable() {
    let area = Rect::new(0, 0, 100, 30);
    let layout = confirmation_layout(area);
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: layout.delete.x,
        row: layout.delete.y,
        modifiers: KeyModifiers::NONE,
    });
    assert!(matches!(
        confirmation_event(&event, area, &mut ConfirmChoice::Cancel),
        ConfirmationEvent::Delete
    ));
}
