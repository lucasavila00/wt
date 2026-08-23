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
