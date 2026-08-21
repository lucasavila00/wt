use anyhow::{Context as _, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use termwiz::input::{KeyCode as TermKeyCode, KeyCodeEncodeModes, KeyboardEncoding, Modifiers};

pub(super) fn encode_key(key: KeyEvent, application_cursor: bool) -> Result<Option<Vec<u8>>> {
    let Some((code, implied)) = key_code(key.code) else {
        return Ok(None);
    };
    let modifiers = modifiers(key.modifiers) | implied;
    let encoded = code
        .encode(
            modifiers,
            KeyCodeEncodeModes {
                encoding: KeyboardEncoding::Xterm,
                application_cursor_keys: application_cursor,
                newline_mode: false,
                modify_other_keys: None,
            },
            true,
        )
        .context("encode terminal key")?;
    Ok(Some(encoded.into_bytes()))
}

pub(super) fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.as_bytes().to_vec()
    }
}

pub(super) fn encode_mouse(
    event: MouseEvent,
    mode: vt100::MouseProtocolMode,
    encoding: vt100::MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    use vt100::MouseProtocolMode::{AnyMotion, ButtonMotion, Press, PressRelease};

    let (button, release) = match event.kind {
        MouseEventKind::Down(button) => (button_code(button), false),
        MouseEventKind::Up(button) if mode != Press => (button_code(button), true),
        MouseEventKind::Up(_) | MouseEventKind::Drag(_) | MouseEventKind::Moved => return None,
        MouseEventKind::ScrollDown
        | MouseEventKind::ScrollUp
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => return None,
    };
    if !matches!(mode, Press | PressRelease | ButtonMotion | AnyMotion) {
        debug_assert_eq!(mode, vt100::MouseProtocolMode::None);
        return None;
    }
    let modifiers = mouse_modifiers(event.modifiers);
    let button = button + modifiers;
    Some(match encoding {
        vt100::MouseProtocolEncoding::Sgr => format!(
            "\x1b[<{button};{};{}{}",
            event.column.saturating_add(1),
            event.row.saturating_add(1),
            if release { 'm' } else { 'M' }
        )
        .into_bytes(),
        vt100::MouseProtocolEncoding::Default => legacy_mouse(
            if release { 3 + modifiers } else { button },
            event.column,
            event.row,
            false,
        ),
        vt100::MouseProtocolEncoding::Utf8 => legacy_mouse(
            if release { 3 + modifiers } else { button },
            event.column,
            event.row,
            true,
        ),
    })
}

fn button_code(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

fn mouse_modifiers(modifiers: KeyModifiers) -> u8 {
    let mut encoded = 0;
    if modifiers.contains(KeyModifiers::SHIFT) {
        encoded |= 4;
    }
    if modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META) {
        encoded |= 8;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        encoded |= 16;
    }
    encoded
}

fn legacy_mouse(button: u8, column: u16, row: u16, utf8: bool) -> Vec<u8> {
    let mut encoded = b"\x1b[M".to_vec();
    if utf8 {
        for value in [u32::from(button), u32::from(column) + 1, u32::from(row) + 1] {
            let character = char::from_u32(32 + value).unwrap_or('\u{fffd}');
            let mut bytes = [0; 4];
            encoded.extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
        }
    } else {
        encoded.extend([32 + button, 32 + coordinate(column), 32 + coordinate(row)]);
    }
    encoded
}

fn coordinate(value: u16) -> u8 {
    u8::try_from(value.saturating_add(1).min(223)).expect("mouse coordinate is bounded")
}

fn key_code(code: KeyCode) -> Option<(TermKeyCode, Modifiers)> {
    let plain = Modifiers::NONE;
    Some(match code {
        KeyCode::Backspace => (TermKeyCode::Backspace, plain),
        KeyCode::Enter => (TermKeyCode::Enter, plain),
        KeyCode::Left => (TermKeyCode::LeftArrow, plain),
        KeyCode::Right => (TermKeyCode::RightArrow, plain),
        KeyCode::Up => (TermKeyCode::UpArrow, plain),
        KeyCode::Down => (TermKeyCode::DownArrow, plain),
        KeyCode::Home => (TermKeyCode::Home, plain),
        KeyCode::End => (TermKeyCode::End, plain),
        KeyCode::PageUp => (TermKeyCode::PageUp, plain),
        KeyCode::PageDown => (TermKeyCode::PageDown, plain),
        KeyCode::Tab => (TermKeyCode::Tab, plain),
        KeyCode::BackTab => (TermKeyCode::Tab, Modifiers::SHIFT),
        KeyCode::Delete => (TermKeyCode::Delete, plain),
        KeyCode::Insert => (TermKeyCode::Insert, plain),
        KeyCode::F(number) => (TermKeyCode::Function(number), plain),
        KeyCode::Char(character) => (TermKeyCode::Char(character), plain),
        KeyCode::Null => (TermKeyCode::Char('\0'), plain),
        KeyCode::Esc => (TermKeyCode::Escape, plain),
        KeyCode::CapsLock => (TermKeyCode::CapsLock, plain),
        KeyCode::ScrollLock => (TermKeyCode::ScrollLock, plain),
        KeyCode::NumLock => (TermKeyCode::NumLock, plain),
        KeyCode::PrintScreen => (TermKeyCode::PrintScreen, plain),
        KeyCode::Pause => (TermKeyCode::Pause, plain),
        KeyCode::Menu => (TermKeyCode::Menu, plain),
        KeyCode::KeypadBegin | KeyCode::Media(_) | KeyCode::Modifier(_) => return None,
    })
}

fn modifiers(source: KeyModifiers) -> Modifiers {
    let mut target = Modifiers::NONE;
    if source.contains(KeyModifiers::SHIFT) {
        target |= Modifiers::SHIFT;
    }
    if source.intersects(KeyModifiers::ALT | KeyModifiers::META) {
        target |= Modifiers::ALT;
    }
    if source.contains(KeyModifiers::CONTROL) {
        target |= Modifiers::CTRL;
    }
    if source.intersects(KeyModifiers::SUPER | KeyModifiers::HYPER) {
        target |= Modifiers::SUPER;
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn arrows_follow_the_remote_cursor_mode() {
        let normal = encode_key(key(KeyCode::Left, KeyModifiers::NONE), false).unwrap();
        let application = encode_key(key(KeyCode::Left, KeyModifiers::NONE), true).unwrap();

        assert_eq!(normal.unwrap(), b"\x1b[D");
        assert_eq!(application.unwrap(), b"\x1bOD");
    }

    #[test]
    fn encodes_control_and_alt_characters() {
        let control = encode_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL), false).unwrap();
        let alt = encode_key(key(KeyCode::Char('x'), KeyModifiers::ALT), false).unwrap();

        assert_eq!(control.unwrap(), b"\x03");
        assert_eq!(alt.unwrap(), b"\x1bx");
    }

    #[test]
    fn wraps_bracketed_paste() {
        assert_eq!(encode_paste("hello", true), b"\x1b[200~hello\x1b[201~");
        assert_eq!(encode_paste("hello", false), b"hello");
    }

    #[test]
    fn encodes_sgr_clicks_and_ignores_motion() {
        let down = mouse(MouseEventKind::Down(MouseButton::Left), 4, 2);
        let up = mouse(MouseEventKind::Up(MouseButton::Left), 4, 2);

        assert_eq!(
            encode_mouse(
                down,
                vt100::MouseProtocolMode::PressRelease,
                vt100::MouseProtocolEncoding::Sgr,
            )
            .unwrap(),
            b"\x1b[<0;5;3M"
        );
        assert_eq!(
            encode_mouse(
                up,
                vt100::MouseProtocolMode::PressRelease,
                vt100::MouseProtocolEncoding::Sgr,
            )
            .unwrap(),
            b"\x1b[<0;5;3m"
        );
        assert_eq!(
            encode_mouse(
                mouse(MouseEventKind::Moved, 5, 3),
                vt100::MouseProtocolMode::AnyMotion,
                vt100::MouseProtocolEncoding::Sgr,
            ),
            None
        );
    }

    #[test]
    fn sends_only_presses_in_press_mode() {
        assert!(encode_mouse(
            mouse(MouseEventKind::Down(MouseButton::Right), 0, 0),
            vt100::MouseProtocolMode::Press,
            vt100::MouseProtocolEncoding::Default,
        )
        .is_some());
        assert_eq!(
            encode_mouse(
                mouse(MouseEventKind::Up(MouseButton::Right), 0, 0),
                vt100::MouseProtocolMode::Press,
                vt100::MouseProtocolEncoding::Default,
            ),
            None
        );
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }
}
