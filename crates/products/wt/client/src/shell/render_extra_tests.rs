use super::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn selected_card_border_uses_the_navigation_color() {
    assert_eq!(selected_card_border_style(true).fg, Some(Color::Blue));
    assert_eq!(selected_card_border_style(false).fg, None);
}

pub(super) fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[test]
fn refresh_titles_distinguish_waiting_from_applied_snapshots() {
    let mut status = crate::shell::refresh_status::RefreshStatus::default();
    assert_eq!(status.title("Codex sessions"), "Codex sessions · Updating…");
    status.finish(Ok("2026-08-21T20:00:00Z".into()));
    assert_eq!(
        status.title("Codex sessions"),
        "Codex sessions · Last updated 2026-08-21T20:00:00Z"
    );
    status.set_failures(vec![
        "context ars could not be queried: connection timed out".into(),
    ]);
    assert_eq!(
        status.title("Codex sessions"),
        "Codex sessions · Last updated 2026-08-21T20:00:00Z"
    );
    assert_eq!(
        status.failure().as_deref(),
        Some(" · Sync failed: context ars could not be queried: connection timed out")
    );
}

#[test]
fn worlds_refresh_title_surfaces_failure_and_preserves_last_success() {
    let mut state = ControlState::default();
    state.finish_worlds_refresh(Ok("2026-08-22T19:29:38Z".into()));
    state.finish_worlds_refresh(Err(vec![
        "context ars could not be queried: request timed out after 60s".into(),
        "context lab could not be queried: connection refused".into(),
    ]));
    assert_eq!(
        state.worlds_refresh().title("Worlds"),
        "Worlds · Last updated 2026-08-22T19:29:38Z"
    );
    assert_eq!(
        state.worlds_refresh().failure().as_deref(),
        Some(" · Sync failed: context ars could not be queried: request timed out after 60s; context lab could not be queried: connection refused")
    );

    state.finish_worlds_refresh(Ok("2026-08-22T19:30:00Z".into()));
    assert_eq!(
        state.worlds_refresh().title("Worlds"),
        "Worlds · Last updated 2026-08-22T19:30:00Z"
    );
}

#[test]
fn running_world_without_a_codex_session_is_a_warning() {
    let area = Rect::new(0, 0, 80, 12);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = ShellModel::new(vec![crate::shell::ShellWorld::test("local.idle", 1)]);
    model.show_worlds();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::screen_tracker::CodexScreenTracker::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(rendered.contains("RUNNING"));

    model.set_codex(Vec::new(), "2026-08-23T12:00:00Z".into(), area);
    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::screen_tracker::CodexScreenTracker::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    let title = terminal.backend().buffer().cell((6, 0)).unwrap().style();
    assert_eq!(title.fg, Some(Color::Yellow));
    assert!(title.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn world_card_has_a_top_right_menu_button() {
    let area = Rect::new(0, 0, 120, 12);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = ShellModel::new(vec![crate::shell::ShellWorld::test("ars.calm-wombat", 1)]);
    model.show_worlds();
    model.set_codex(Vec::new(), "2026-08-23T12:00:00Z".into(), area);

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::screen_tracker::CodexScreenTracker::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    let card = card_grid(area, 0, 1, WORLD_CARD_HEIGHT)
        .card_rect(0)
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rendered = (card.y..card.bottom())
        .map(|row| {
            (card.x..card.right())
                .map(|column| buffer.cell((column, row)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @"
    ┌ 󰚩 IDLE · NO ACTIVE CODEX SESSION ───────────── … Menu ┐
    │ars.calm-wombat                                        │
    │2 CPU · 4G · 1G/32G disk                               │
    │                                                       │
    │                                                       │
    │                                                       │
    │                                                       │
    │                                                       │
    │Enter or click to open                                 │
    └───────────────────────────────────────────────────────┘
    ");
}

#[test]
fn world_menu_matches_the_command_palette_style() {
    let area = Rect::new(0, 0, 120, 12);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = ShellModel::new(vec![crate::shell::ShellWorld::test("ars.calm-wombat", 1)]);
    model.show_worlds();
    let card = card_grid(area, 0, 1, WORLD_CARD_HEIGHT)
        .card_rect(0)
        .unwrap();
    model.handle_mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: card.right() - 2,
            row: card.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        },
        area,
    );

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::screen_tracker::CodexScreenTracker::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    let (menu, _, _, results, _) = super::super::world_menu::menu_layout(area);
    let buffer = terminal.backend().buffer();
    let rendered = (menu.y..menu.bottom())
        .map(|row| {
            (menu.x..menu.right())
                .map(|column| buffer.cell((column, row)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @"
    ┌World Menu──────────────────────────────────────────────────────────┐
    │ars.calm-wombat                                                     │
    │────────────────────────────────────────────────────────────────────│
    │ Delete                                                            │
    │                                                                    │
    │                                                                    │
    │                                                                    │
    │Enter run · Esc close                                               │
    └────────────────────────────────────────────────────────────────────┘
    ");
    assert!(buffer
        .cell((results.x + 2, results.y))
        .unwrap()
        .style()
        .add_modifier
        .contains(Modifier::REVERSED));
}
