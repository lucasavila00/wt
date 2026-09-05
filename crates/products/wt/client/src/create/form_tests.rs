use super::*;
use ratatui::{backend::TestBackend, Terminal};
use wt_client::config::{Context, ContextKind};

fn form() -> Form {
    Form::new(
        &ClientConfig {
            contexts: vec![
                Context {
                    name: "local".into(),
                    kind: ContextKind::BareMetalLocal,
                },
                Context {
                    name: "lab".into(),
                    kind: ContextKind::BareMetalLocal,
                },
            ],
        },
        GitAuthor {
            name: "Test User".into(),
            email: "test@example.com".into(),
        },
        &std::collections::BTreeSet::new(),
        std::collections::BTreeMap::new(),
    )
    .unwrap()
}

#[test]
fn host_form_validates_and_builds_the_request_input() {
    let mut form = form();
    form.name = "demo".into();
    form.context = 1;
    let input = form.input().unwrap();
    assert_eq!(input.context, "lab");
    assert_eq!(input.name.as_str(), "demo");
    assert_eq!(input.vcpus, DEFAULT_VCPUS);
    assert_eq!(input.git_user_name, "Test User");
}

#[test]
fn suggests_the_first_unused_world_name() {
    let used_names = ["amber-badger".to_owned(), "amber-bison".to_owned()]
        .into_iter()
        .collect();
    assert_eq!(suggested_name_from(&used_names, 0), "amber-corgi");
}

#[test]
fn invalid_values_stay_in_the_form() {
    let mut form = form();
    form.focus = 1;
    form.name.clear();
    form.name_is_suggestion = false;
    assert!(matches!(
        form.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Action::None
    ));
    assert!(form.error.as_deref().unwrap().contains("world name"));
    assert_eq!(form.focus, 1);
}

#[test]
fn starts_on_ok_and_accepts_the_valid_defaults() {
    let mut form = form();
    assert_eq!(form.focus, OK_FOCUS);
    assert!(matches!(
        form.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Action::None
    ));
    assert_eq!(form.stage, Stage::Review);
}

#[test]
fn arrows_step_cpu_count_by_one() {
    let mut form = form();
    form.focus = 2;
    form.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(form.vcpus, "3");
    form.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    form.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(form.vcpus, "1");
}

#[test]
fn arrows_step_memory_in_powers_of_two_gibibytes() {
    let mut form = form();
    form.focus = 3;
    form.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(form.memory, "8192");
    form.memory = "6144".into();
    form.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(form.memory, "4096");
}

#[test]
fn arrows_step_disk_in_powers_of_two_gibibytes() {
    let mut form = form();
    form.focus = 4;
    form.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(form.disk, "16");
    form.disk = "24".into();
    form.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(form.disk, "32");
}

#[test]
fn arrows_do_not_exceed_the_selected_context_capacity() {
    let mut form = form();
    form.capacities.insert(
        "local".into(),
        ResourceCapacity {
            reserved: Default::default(),
            total: wt_control_protocol::Resources {
                vcpus: 8,
                memory_mib: 12 * 1024,
                disk_gib: 96,
            },
        },
    );
    form.focus = 3;
    form.memory = (8 * 1024).to_string();
    form.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(form.memory, "8192");
    form.focus = 4;
    form.disk = "64".into();
    form.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(form.disk, "64");
}

#[test]
fn ok_button_is_directly_clickable() {
    let mut form = form();
    let area = Rect::new(0, 0, 100, 30);
    let button = form_layout(area, HOST_FIELDS.len()).fields;
    assert!(matches!(
        form.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: button.x + 3,
                row: button.y + OK_FOCUS as u16,
                modifiers: KeyModifiers::NONE,
            },
            area,
        ),
        Action::None
    ));
    assert_eq!(form.stage, Stage::Review);
}

#[test]
fn clicking_a_field_moves_keyboard_focus_to_it() {
    let mut form = form();
    let area = Rect::new(0, 0, 100, 30);
    let fields = form_layout(area, HOST_FIELDS.len()).fields;
    let _ = form.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: fields.x,
            row: fields.y + 1,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(form.focus, 1);
}

#[test]
fn terminal_sequence_reaches_confirmation() {
    let mut form = form();
    let mut action = Action::None;
    for character in "\nrepo-feature\n\n\n\n\n".chars() {
        let code = if character == '\n' {
            KeyCode::Enter
        } else {
            KeyCode::Char(character)
        };
        action = form.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    assert!(matches!(action, Action::Submit(_)));
}

#[test]
fn overlay_clears_the_modal_background() {
    let backend = TestBackend::new(84, 22);
    let mut terminal = Terminal::new(backend).unwrap();
    let form = form();
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new("x".repeat(84 * 22)), frame.area());
            form.render_overlay(frame, frame.area());
        })
        .unwrap();
    let modal = form_layout(Rect::new(0, 0, 84, 22), HOST_FIELDS.len()).modal;
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "x");
    assert_eq!(buffer[(modal.x + 1, modal.y + 1)].symbol(), " ");
}
