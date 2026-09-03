use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use uuid::Uuid;

#[test]
fn button_does_not_open_card_and_deletes_by_stable_identity() {
    let area = area();
    let mut model = ShellModel::new(vec![world("one"), world("two")]);
    model.show_worlds();
    let second = card(&model, area, 1);

    let (changed, route) = model.handle_mouse(mouse(second.right() - 2, second.y), area);

    assert!(changed);
    assert_eq!(route, Some(InputRoute::Consumed));
    assert_eq!(model.mode(), Mode::Control);
    assert_eq!(model.active_world(), "two");
    assert!(model.world_menu().is_some());

    let selected = model.worlds()[1].clone();
    model.reconcile_worlds(vec![selected.clone(), world("three")]);
    model.handle_key(key(KeyCode::Down), area);
    let InputRoute::DeleteWorld(world) = model.handle_key(key(KeyCode::Enter), area) else {
        panic!("world menu did not request deletion");
    };
    assert_eq!(*world, selected);
    assert!(model.world_menu().is_none());
}

#[test]
fn supports_mouse_selection_and_escape() {
    let area = area();
    let mut model = ShellModel::new(vec![world("one")]);
    model.show_worlds();
    let card = card(&model, area, 0);
    model.handle_mouse(mouse(card.right() - 2, card.y), area);

    assert_eq!(
        model.handle_key(key(KeyCode::Esc), area),
        InputRoute::Consumed
    );
    assert!(model.world_menu().is_none());

    model.handle_mouse(mouse(card.right() - 2, card.y), area);
    let result = super::super::world_menu::menu_result_area(area);
    let (_, route) = model.handle_mouse(mouse(result.x, result.y + 1), area);
    let Some(InputRoute::DeleteWorld(world)) = route else {
        panic!("clicking Delete did not request deletion");
    };
    assert_eq!(world.name, "one");
}

fn card(model: &ShellModel, area: Rect, index: usize) -> Rect {
    super::super::control::card_grid(
        area,
        model.control().world_scroll(),
        model.world_count(),
        super::super::control::WORLD_CARD_HEIGHT,
    )
    .card_rect(index)
    .unwrap()
}

fn world(name: &str) -> ShellWorld {
    ShellWorld::test(name, Uuid::new_v4().as_u128())
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
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
    Rect::new(0, 0, 80, 24)
}
