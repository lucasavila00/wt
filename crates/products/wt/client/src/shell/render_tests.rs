use super::*;
use crate::shell::bar;
use crate::shell::codex::ShellWorld;
use crate::shell::control::{CodexCardIdentity, CodexCardKind};
use crate::shell::model::InputRoute;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Position;
use ratatui::{backend::TestBackend, Terminal};
use uuid::Uuid;
use wt_control_protocol::{ByobuTarget, CodexSessionState};

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
                git_context_health: None,
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
fn failed_codex_refresh_is_shown_in_the_red_footer() {
    let area = Rect::new(0, 0, 160, 18);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    model.set_codex(Vec::new(), "2026-08-22T19:25:06Z".into(), area);
    model.control_mut().set_context_failures(vec![
        "context ars could not be queried: connection timed out".into(),
    ]);
    press(&mut model, KeyCode::Tab, area);

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

    let buffer = terminal.backend().buffer();
    let rendered = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(
        rendered.contains("Sync failed: context ars could not be queried: connection timed out")
    );
    assert!(buffer.content().iter().any(|cell| cell.fg == Color::Red));
}

#[test]
fn failed_worlds_refresh_is_shown_in_the_red_footer() {
    let area = Rect::new(0, 0, 160, 18);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut model = model(&["ars.dev"]);
    model.show_worlds();
    model.finish_worlds_refresh(Ok("2026-08-22T19:25:06Z".into()));
    model.finish_worlds_refresh(Err(vec![
        "context ars could not be queried: connection timed out".into(),
    ]));

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

    let buffer = terminal.backend().buffer();
    let rendered = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(
        rendered.contains("Sync failed: context ars could not be queried: connection timed out")
    );
    assert!(buffer.content().iter().any(|cell| cell.fg == Color::Red));
}

#[test]
fn live_session_repository_is_card_chrome() {
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
                repository_root: Some("/home/wt/wt".into()),
                repository_url: Some("git@github.com:lucasavila00/wt.git".into()),
                git_branch: Some("wt/live".into()),
                git_context_health: None,
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
    live_parser.process(b"world output");

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

    let buffer = terminal.backend().buffer();
    let row = |y| {
        (5..52)
            .map(|x| buffer.cell((x, y)).unwrap().symbol())
            .collect::<String>()
    };
    assert_eq!(row(1), "│world output                                 │");
    assert_eq!(row(13), "└───────────────────── github:lucasavila00/wt ┘");
}
