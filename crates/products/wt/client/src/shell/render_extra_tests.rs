use super::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn refresh_titles_distinguish_waiting_from_applied_snapshots() {
    assert_eq!(
        refresh_title("Codex sessions", None, None),
        "Codex sessions · Updating…"
    );
    assert_eq!(
        refresh_title("Codex sessions", Some("2026-08-21T20:00:00Z"), None),
        "Codex sessions · Last updated 2026-08-21T20:00:00Z"
    );
    assert_eq!(
        refresh_title(
            "Codex sessions",
            Some("2026-08-21T20:00:00Z"),
            Some(&["context ars could not be queried: connection timed out".into()])
        ),
        "Codex sessions · Last updated 2026-08-21T20:00:00Z · Sync failed: context ars could not be queried: connection timed out"
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
                &super::super::preview::PreviewSet::new(),
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
