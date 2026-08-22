use super::*;
use crate::shell::codex::ShellWorld;
use crate::shell::control::{CodexCardIdentity, CodexCardKind};
use crate::shell::model::InputRoute;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Position;
use ratatui::{backend::TestBackend, Terminal};
use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState, InstanceStatus};

fn parser() -> vt100::Parser {
    let mut parser = vt100::Parser::new(6, 80, 0);
    parser.process(b"world output\r\n\x1b[31mred\x1b[0m");
    parser
}

fn model(names: &[&str]) -> ShellModel {
    ShellModel::new(
        names
            .iter()
            .enumerate()
            .map(|(index, name)| ShellWorld::test(name, index as u128 + 1))
            .collect(),
    )
}

fn press(model: &mut ShellModel, code: KeyCode, area: Rect) {
    model.handle_key(
        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
        area,
    );
}

#[test]
fn switcher_activates_the_world_bar() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one", "local.two"]);
    press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
    press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_switcher_world_bar", terminal.backend().buffer());
    let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
    assert_eq!(brand.fg, Some(Color::Reset));
    assert_eq!(brand.bg, Some(Color::Reset));
    assert!(brand
        .add_modifier
        .contains(Modifier::BOLD | Modifier::REVERSED));
    let style = terminal.backend().buffer().cell((6, 0)).unwrap().style();
    assert_eq!(style.fg, Some(Color::Reset));
    assert_eq!(style.bg, Some(Color::Reset));
    assert!(style
        .add_modifier
        .contains(Modifier::BOLD | Modifier::REVERSED));
}

#[test]
fn world_bar_uses_reverse_video() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one", "local.two"]);
    press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(3, 2));
    insta::assert_debug_snapshot!("shell_inactive_world_bar", terminal.backend().buffer());
    let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
    assert_eq!(brand.fg, Some(Color::Reset));
    assert_eq!(brand.bg, Some(Color::Reset));
    assert!(brand
        .add_modifier
        .contains(Modifier::BOLD | Modifier::REVERSED));
    let style = terminal.backend().buffer().cell((6, 0)).unwrap().style();
    assert_eq!(style.fg, Some(Color::Reset));
    assert_eq!(style.bg, Some(Color::Reset));
    assert!(style
        .add_modifier
        .contains(Modifier::BOLD | Modifier::REVERSED));
}

#[test]
fn disabled_f5_override_emphasizes_the_top_bar() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = ShellModel::new(vec!["local.one".into()]);
    model.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::F(5), crossterm::event::KeyModifiers::SHIFT),
        Rect::new(0, 0, 80, 6),
    );
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_disabled_f5_override", terminal.backend().buffer());
}

#[test]
fn test_server_warning_owns_the_topbar_in_control_and_world_views() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one"]);
    model.set_test_server(true);

    terminal
        .draw(|frame| draw(frame, None, None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!(
        "shell_test_server_control_warning",
        terminal.backend().buffer()
    );
    let style = terminal.backend().buffer().cell((79, 0)).unwrap().style();
    assert_eq!(style.fg, Some(Color::Reset));
    assert_eq!(style.bg, Some(Color::Reset));
    assert!(style
        .add_modifier
        .contains(Modifier::BOLD | Modifier::REVERSED));

    press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 12));
    let parser = parser();
    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();
    let top = (0..80)
        .map(|column| {
            terminal
                .backend()
                .buffer()
                .cell((column, 0))
                .unwrap()
                .symbol()
        })
        .collect::<String>();
    assert!(top.contains("WT E2E TEST SERVER"));
}

#[test]
fn closed_session_uses_a_reverse_video_reconnect_bar() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = ShellModel::new(vec!["local.one".into()]);
    model.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::F(5), crossterm::event::KeyModifiers::NONE),
        Rect::new(0, 0, 80, 6),
    );
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                Some(parser.screen()),
                Some("SSH session ended: Exited with code 255"),
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_closed_session", terminal.backend().buffer());
    let status = terminal.backend().buffer().cell((0, 5)).unwrap().style();
    assert_eq!(status.fg, Some(Color::Reset));
    assert_eq!(status.bg, Some(Color::Reset));
    assert!(status
        .add_modifier
        .contains(Modifier::BOLD | Modifier::REVERSED));
}

#[test]
fn control_ui_has_activity_scaffolding() {
    let backend = TestBackend::new(64, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let model = model(&["local.one"]);
    assert_eq!(model.mode(), Mode::Control);
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_activities", terminal.backend().buffer());
}

#[test]
fn control_ui_shows_world_cards() {
    let backend = TestBackend::new(100, 17);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev", "lab.broken"]);
    model.worlds_mut()[0].resources = "4 CPU · 8G · 12.3G/64G disk".into();
    model.worlds_mut()[0].detail = "2 wt-tools reports; run `wt reports`".into();
    model.worlds_mut()[1].status = InstanceStatus::Error;
    model.worlds_mut()[1].resources = "2 CPU · 4G · 8G/32G disk".into();
    model.worlds_mut()[1].detail = "host preparation failed; run `wt rm lab.broken`".into();
    model.set_worlds_updated_at("2026-08-21T23:26:52Z".into());
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 100, 17));
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_world_cards", terminal.backend().buffer());
}

#[test]
fn control_ui_opens_the_command_palette() {
    let backend = TestBackend::new(64, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one"]);
    press(&mut model, KeyCode::F(1), Rect::new(0, 0, 64, 16));
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_command_palette", terminal.backend().buffer());
}

#[test]
fn control_ui_shows_codex_session_cards() {
    let backend = TestBackend::new(100, 22);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    let session_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let world_id = Uuid::parse_str("223e4567-e89b-12d3-a456-426614174000").unwrap();
    let unknown_session_id = Uuid::parse_str("223e4567-e89b-12d3-a456-426614174002").unwrap();
    let unknown_world_id = Uuid::parse_str("223e4567-e89b-12d3-a456-426614174003").unwrap();
    let target = ByobuTarget {
        tmux_session: "wt-host".into(),
        pane_id: "%1".into(),
    };
    model.set_codex(
        vec![
            CodexCard {
                identity: CodexCardIdentity::Observation {
                    context: "ars".into(),
                    session_id,
                    world_id,
                    tmux_session: target.tmux_session.clone(),
                    pane_id: target.pane_id.clone(),
                },
                context: "ars".into(),
                session_id: Some(session_id),
                timestamp: Some(now_ms()),
                title: Some("Improve authentication diagnostics".into()),
                kind: CodexCardKind::Observation {
                    world_id,
                    world_name: "dev".into(),
                    cwd: "/home/wt/project".into(),
                    repository_root: Some("/home/wt/project".into()),
                    repository_url: Some("git@github.com:acme/project.git".into()),
                    git_branch: Some("wt/auth-diagnostics".into()),
                    state: CodexSessionState::NeedsAttention,
                    session_start_source: None,
                    target,
                },
            },
            CodexCard {
                identity: CodexCardIdentity::Observation {
                    context: "ars".into(),
                    session_id: unknown_session_id,
                    world_id: unknown_world_id,
                    tmux_session: "wt-host".into(),
                    pane_id: "%2".into(),
                },
                context: "ars".into(),
                session_id: Some(unknown_session_id),
                timestamp: Some(now_ms()),
                title: Some("Investigate compacted session".into()),
                kind: CodexCardKind::Observation {
                    world_id: unknown_world_id,
                    world_name: "compact".into(),
                    cwd: "/home/wt/project".into(),
                    repository_root: None,
                    repository_url: None,
                    git_branch: None,
                    state: CodexSessionState::Unknown,
                    session_start_source: Some("compact".into()),
                    target: ByobuTarget {
                        tmux_session: "wt-host".into(),
                        pane_id: "%2".into(),
                    },
                },
            },
            CodexCard::rollout_only(
                "ars",
                Uuid::parse_str("323e4567-e89b-12d3-a456-426614174000").unwrap(),
                now_ms(),
                Some("Review saved migration work".into()),
            ),
            CodexCard::context_error("lab", "context lab: SSH failed".into()),
        ],
        "2026-08-21T20:00:00Z".into(),
        Rect::new(0, 0, 100, 22),
    );
    let parser = parser();

    terminal
        .draw(|frame| draw(frame, Some(parser.screen()), None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_codex_sessions", terminal.backend().buffer());
}

#[test]
fn failed_codex_open_is_a_retryable_toast_without_internal_details() {
    let backend = TestBackend::new(80, 18);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    let session_id = Uuid::from_u128(10);
    let world_id = Uuid::from_u128(20);
    let target = ByobuTarget {
        tmux_session: "wt-host".into(),
        pane_id: "%1".into(),
    };
    model.set_codex(
        vec![CodexCard {
            identity: CodexCardIdentity::Observation {
                context: "ars".into(),
                session_id,
                world_id,
                tmux_session: target.tmux_session.clone(),
                pane_id: target.pane_id.clone(),
            },
            context: "ars".into(),
            session_id: Some(session_id),
            timestamp: Some(now_ms()),
            title: Some("Focus the session".into()),
            kind: CodexCardKind::Observation {
                world_id,
                world_name: "dev".into(),
                cwd: "/home/wt/project".into(),
                repository_root: Some("/home/wt/project".into()),
                repository_url: Some("https://github.com/lucasavila00/wt".into()),
                git_branch: Some("wt/ctx-timeout-toast".into()),
                state: CodexSessionState::Unknown,
                session_start_source: Some("compact".into()),
                target,
            },
        }],
        "2026-08-21T20:00:00Z".into(),
        Rect::new(0, 0, 80, 18),
    );
    let InputRoute::OpenCodex(target) = model.handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        Rect::new(0, 0, 80, 18),
    ) else {
        panic!("live card did not produce an open target");
    };
    model.finish_codex_open(&target, None, true);

    terminal
        .draw(|frame| draw(frame, None, None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!(
        "shell_codex_open_failure_toast",
        terminal.backend().buffer()
    );
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(!rendered.contains("world_id"));
    let (retry, dismiss) = super::super::toast::actions(Rect::new(0, 0, 80, 18));
    assert!(terminal
        .backend()
        .buffer()
        .cell((retry.right() - 1, retry.y))
        .unwrap()
        .style()
        .add_modifier
        .contains(Modifier::BOLD));
    assert!(terminal
        .backend()
        .buffer()
        .cell((dismiss.x, dismiss.y))
        .unwrap()
        .style()
        .add_modifier
        .contains(Modifier::BOLD));
}

#[test]
fn failed_context_refresh_is_a_sanitized_retryable_toast() {
    let backend = TestBackend::new(80, 18);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    model.set_codex_context_failures(vec!["ars".into()]);

    terminal
        .draw(|frame| draw(frame, None, None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!(
        "shell_codex_context_failure_toast",
        terminal.backend().buffer()
    );
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(!rendered.contains("Permission denied"));
    assert!(!rendered.contains("/home/wt/.codex"));
}

#[test]
fn refresh_titles_distinguish_waiting_from_applied_snapshots() {
    assert_eq!(
        refresh_title("Codex sessions", None),
        "Codex sessions · Updating…"
    );
    assert_eq!(
        refresh_title("Codex sessions", Some("2026-08-21T20:00:00Z")),
        "Codex sessions · Last updated 2026-08-21T20:00:00Z"
    );
}

#[test]
fn empty_shell_renders_the_control_ui() {
    let backend = TestBackend::new(64, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let model = ShellModel::new(Vec::new());

    terminal
        .draw(|frame| draw(frame, None, None, &model, None, None, None))
        .unwrap();

    insta::assert_debug_snapshot!("shell_empty_control", terminal.backend().buffer());
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}
