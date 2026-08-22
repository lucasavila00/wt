use super::*;
use uuid::Uuid;

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

fn area() -> Rect {
    Rect::new(0, 0, 80, 24)
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
fn active_navbar_opens_and_drives_the_command_palette() {
    for key_code in [KeyCode::F(1), KeyCode::Char('1')] {
        let mut model = model();
        model.handle_key(key(KeyCode::F(5)), area());

        assert_eq!(
            model.handle_key(key(key_code), area()),
            InputRoute::Consumed
        );
        assert!(model.control().palette().is_open());
        assert_eq!(
            model.handle_key(key(KeyCode::Char('n')), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.control().palette().query(), "n");
        assert_eq!(
            model.handle_key(key(KeyCode::Left), area()),
            InputRoute::Consumed
        );
        assert_eq!(model.active(), 0);
        assert_eq!(
            model.handle_key(key(KeyCode::Esc), area()),
            InputRoute::Consumed
        );
        assert!(!model.control().palette().is_open());
        assert_eq!(model.mode(), Mode::Switcher);
    }
}

#[test]
fn active_navbar_runs_commands_without_leaving_the_world() {
    let mut model = model();
    model.handle_key(key(KeyCode::F(5)), area());
    model.handle_key(key(KeyCode::F(1)), area());
    for character in "delete".chars() {
        model.handle_key(key(KeyCode::Char(character)), area());
    }

    assert_eq!(
        model.handle_key(key(KeyCode::Enter), area()),
        InputRoute::Command(ControlCommand::DeleteWorld)
    );
    assert_eq!(model.mode(), Mode::Switcher);
}

#[test]
fn shell_starts_in_control_mode() {
    let model = ShellModel::new(vec![world("one")]);

    assert_eq!(model.mode(), Mode::Control);
}
