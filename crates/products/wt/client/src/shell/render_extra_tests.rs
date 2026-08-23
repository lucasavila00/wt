use super::*;
use ratatui::{backend::TestBackend, Terminal};
use wt_control_protocol::{ResourceCapacity, Resources};

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
        "Codex sessions · Last updated 2026-08-21T20:00:00Z · Sync failed: context ars could not be queried: connection timed out"
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
        "Worlds · Last updated 2026-08-22T19:29:38Z · Sync failed: context ars could not be queried: request timed out after 60s; context lab could not be queried: connection refused"
    );

    state.finish_worlds_refresh(Ok("2026-08-22T19:30:00Z".into()));
    assert_eq!(
        state.worlds_refresh().title("Worlds"),
        "Worlds · Last updated 2026-08-22T19:30:00Z"
    );
}

#[test]
fn empty_shell_renders_the_control_ui() {
    let backend = TestBackend::new(64, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let model = ShellModel::new(Vec::new());

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_empty_control", terminal.backend().buffer());
}

#[test]
fn control_footer_shows_reserved_and_total_resources() {
    let backend = TestBackend::new(160, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = ShellModel::new(vec!["local.one".into()]);
    model.control_mut().set_capacity(ResourceCapacity {
        reserved: Resources {
            vcpus: 6,
            memory_mib: 10_240,
            disk_gib: 68,
        },
        total: Resources {
            vcpus: 16,
            memory_mib: 32_768,
            disk_gib: 256,
        },
    });

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!(
        "shell_control_resource_capacity",
        terminal.backend().buffer()
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
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();
    assert!(format!("{:?}", terminal.backend().buffer()).contains("RUNNING"));

    model.set_codex(Vec::new(), "2026-08-23T12:00:00Z".into(), area);
    terminal
        .draw(|frame| {
            draw(
                frame,
                &[],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_idle_world_warning", terminal.backend().buffer());
    let title = terminal.backend().buffer().cell((6, 0)).unwrap().style();
    assert_eq!(title.fg, Some(Color::Yellow));
    assert!(title.add_modifier.contains(Modifier::BOLD));
}
