use anyhow::{Context as _, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use termwiz::input::{
    KeyCode as TermKeyCode, KeyCodeEncodeModes, KeyboardEncoding, Modifiers,
};

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
}
