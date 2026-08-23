use super::*;
use crate::shell::bar;
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
fn world_bar_is_dimmed_and_shows_control_targets() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one", "local.two"]);
    press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_inactive_world_bar", terminal.backend().buffer());
    let brand = terminal.backend().buffer().cell((0, 0)).unwrap().style();
    assert_eq!(brand.fg, Some(Color::Reset));
    assert_eq!(brand.bg, Some(Color::Reset));
    assert!(brand.add_modifier.contains(Modifier::BOLD | Modifier::DIM));
    let world = bar::world_bar_world(&model, Rect::new(0, 0, 80, 6));
    let brand = bar::world_bar_brand(Rect::new(0, 0, 80, 6));
    let control = bar::world_bar_control(Rect::new(0, 0, 80, 6));
    for clickable in [brand, world, control] {
        let style = terminal
            .backend()
            .buffer()
            .cell((clickable.x, clickable.y))
            .unwrap()
            .style();
        assert!(style.add_modifier.contains(Modifier::BOLD | Modifier::DIM));
    }
}

#[test]
fn world_bar_preserves_the_world_cursor() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one", "local.two"]);
    press(&mut model, KeyCode::F(5), Rect::new(0, 0, 80, 6));
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    assert_eq!(terminal.get_cursor_position().unwrap(), Position::new(3, 2));
}

#[test]
fn test_server_warning_owns_the_topbar_in_control_and_world_views() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one"]);
    model.set_test_server(true);

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
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
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
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
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
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_activities", terminal.backend().buffer());
    assert_eq!(
        terminal.backend().buffer().cell((0, 7)).unwrap().fg,
        Color::Blue
    );
    assert_eq!(
        terminal.backend().buffer().cell((2, 7)).unwrap().fg,
        Color::Blue
    );
    assert_eq!(
        terminal.backend().buffer().cell((2, 1)).unwrap().fg,
        Color::Reset
    );
}

#[test]
fn control_ui_shows_world_cards() {
    let backend = TestBackend::new(100, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev", "lab.broken"]);
    model.worlds_mut()[0].resources = "4 CPU · 8G · 12.3G/64G disk".into();
    model.worlds_mut()[0].detail = "2 wt-tools reports; run `wt reports`".into();
    model.worlds_mut()[1].status = InstanceStatus::Error;
    model.worlds_mut()[1].resources = "2 CPU · 4G · 8G/32G disk".into();
    model.worlds_mut()[1].detail = "host preparation failed; run `wt rm lab.broken`".into();
    let session_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    model.set_codex(
        vec![CodexCard {
            identity: CodexCardIdentity::Observation {
                context: "ars".into(),
                session_id,
                world_id: Uuid::from_u128(1),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
            },
            context: "ars".into(),
            session_id: Some(session_id),
            timestamp: Some(now_ms()),
            latest_user_message: Some("Add checkout details to world cards".into()),
            kind: CodexCardKind::Observation {
                world_id: Uuid::from_u128(1),
                world_name: "dev".into(),
                cwd: "/home/wt/wt".into(),
                repository_root: Some("/home/wt/wt".into()),
                repository_url: Some("git@github.com:acme/wt.git".into()),
                git_branch: Some("wt/world-card-sessions".into()),
                state: CodexSessionState::Working,
                is_compacting: false,
                session_start_source: None,
                target: ByobuTarget {
                    tmux_session: "wt-host".into(),
                    pane_id: "%1".into(),
                },
            },
        }],
        "2026-08-21T23:26:52Z".into(),
        Rect::new(0, 0, 100, 25),
    );
    model.finish_worlds_refresh(Ok("2026-08-21T23:26:52Z".into()));
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 100, 25));
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 100, 25));
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_world_cards", terminal.backend().buffer());
}

#[test]
fn control_ui_opens_the_command_palette() {
    let backend = TestBackend::new(64, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one"]);
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 64, 16));
    press(&mut model, KeyCode::F(1), Rect::new(0, 0, 64, 16));
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_command_palette", terminal.backend().buffer());
}

#[test]
fn control_ui_opens_help() {
    let backend = TestBackend::new(64, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["local.one"]);
    press(&mut model, KeyCode::F(2), Rect::new(0, 0, 64, 16));
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_help", terminal.backend().buffer());
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
                latest_user_message: Some(
                    concat!(
                        "Show the latest authentication failure with enough context to act on it. ",
                        "Include which credential source was selected, which host rejected it, ",
                        "and the recovery command the user can run without exposing any secret ",
                        "values or raw protocol diagnostics in the terminal session card."
                    )
                    .into(),
                ),
                kind: CodexCardKind::Observation {
                    world_id,
                    world_name: "dev".into(),
                    cwd: "/home/wt/project".into(),
                    repository_root: Some("/home/wt/project".into()),
                    repository_url: Some("git@github.com:acme/project.git".into()),
                    git_branch: Some("wt/auth-diagnostics".into()),
                    state: CodexSessionState::NeedsAttention,
                    is_compacting: true,
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
                latest_user_message: Some(
                    "Investigate why this compacted session is unknown".into(),
                ),
                kind: CodexCardKind::Observation {
                    world_id: unknown_world_id,
                    world_name: "compact".into(),
                    cwd: "/home/wt/project".into(),
                    repository_root: None,
                    repository_url: None,
                    git_branch: None,
                    state: CodexSessionState::Unknown,
                    is_compacting: false,
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
                Some("Review the saved migration and identify the remaining steps".into()),
            ),
            CodexCard::context_error("lab", "context lab: SSH failed".into()),
        ],
        "2026-08-21T20:00:00Z".into(),
        Rect::new(0, 0, 100, 22),
    );
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 100, 22));
    let parser = parser();

    terminal
        .draw(|frame| {
            draw(
                frame,
                &[parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                None,
                &model,
                None,
                None,
                None,
            )
        })
        .unwrap();
    insta::assert_debug_snapshot!("shell_control_codex_sessions", terminal.backend().buffer());
}

#[test]
fn control_ui_shows_live_session_panes() {
    let backend = TestBackend::new(100, 18);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    let session_id = Uuid::from_u128(2);
    model.set_codex(
        vec![CodexCard {
            identity: CodexCardIdentity::Observation {
                context: "ars".into(),
                session_id,
                world_id: Uuid::from_u128(1),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
            },
            context: "ars".into(),
            session_id: Some(session_id),
            timestamp: Some(now_ms()),
            latest_user_message: None,
            kind: CodexCardKind::Observation {
                world_id: Uuid::from_u128(1),
                world_name: "dev".into(),
                cwd: "/home/wt/wt".into(),
                repository_root: None,
                repository_url: None,
                git_branch: Some("wt/live".into()),
                state: CodexSessionState::Working,
                is_compacting: false,
                session_start_source: None,
                target: ByobuTarget {
                    tmux_session: "wt-host".into(),
                    pane_id: "%1".into(),
                },
            },
        }],
        "2026-08-22T19:00:00Z".into(),
        Rect::new(0, 0, 100, 18),
    );
    let mut live_parser = vt100::Parser::new(10, 91, 0);
    live_parser.process(b"world output\r\n\x1b[31mred\x1b[0m");

    terminal
        .draw(|frame| {
            super::super::live::draw(
                frame,
                frame.area(),
                &[live_parser.screen()],
                &super::super::live_focus::LiveFocus::default(),
                &model,
            )
        })
        .unwrap();

    insta::assert_debug_snapshot!("shell_control_live_sessions", terminal.backend().buffer());
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
            latest_user_message: Some("Retry opening this session".into()),
            kind: CodexCardKind::Observation {
                world_id,
                world_name: "dev".into(),
                cwd: "/home/wt/project".into(),
                repository_root: Some("/home/wt/project".into()),
                repository_url: Some("https://github.com/lucasavila00/wt".into()),
                git_branch: Some("wt/ctx-timeout-toast".into()),
                state: CodexSessionState::Unknown,
                is_compacting: false,
                session_start_source: Some("compact".into()),
                target,
            },
        }],
        "2026-08-21T20:00:00Z".into(),
        Rect::new(0, 0, 80, 18),
    );
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 80, 18));
    let InputRoute::OpenCodex(target) = model.handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        Rect::new(0, 0, 80, 18),
    ) else {
        panic!("live card did not produce an open target");
    };
    model.finish_codex_open(&target, None, true);

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
fn failed_context_refresh_is_shown_in_the_title() {
    let backend = TestBackend::new(80, 18);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    model.set_codex(
        vec![CodexCard::rollout_only(
            "ars",
            Uuid::from_u128(10),
            now_ms(),
            Some("Keep this session visible while sync is unavailable".into()),
        )],
        "2026-08-22T19:25:06Z".into(),
        Rect::new(0, 0, 80, 18),
    );
    model.set_codex_context_failures(vec![
        "context ars could not be queried: connection timed out".into(),
    ]);
    press(&mut model, KeyCode::Tab, Rect::new(0, 0, 80, 18));

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
        "shell_codex_context_failure_title",
        terminal.backend().buffer()
    );
}
