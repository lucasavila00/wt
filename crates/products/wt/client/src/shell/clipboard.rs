#[derive(Default)]
pub(super) struct ClipboardRelay {
    sequence: Vec<u8>,
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc,
}

impl ClipboardRelay {
    pub(super) fn process(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut writes = Vec::new();
        for &byte in bytes {
            match self.state {
                State::Ground if byte == b'\x1b' => {
                    self.sequence.push(byte);
                    self.state = State::Escape;
                }
                State::Ground => {}
                State::Escape if byte == b']' => {
                    self.sequence.push(byte);
                    self.state = State::Osc;
                }
                State::Escape if byte == b'\x1b' => {
                    self.sequence.clear();
                    self.sequence.push(byte);
                }
                State::Escape => {
                    self.sequence.clear();
                    self.state = State::Ground;
                }
                State::Osc => {
                    self.sequence.push(byte);
                    let terminated = byte == b'\x07'
                        || (byte == b'\\'
                            && self.sequence.get(self.sequence.len().saturating_sub(2))
                                == Some(&b'\x1b'));
                    if terminated {
                        if is_clipboard_write(&self.sequence) {
                            writes.push(std::mem::take(&mut self.sequence));
                        } else {
                            self.sequence.clear();
                        }
                        self.state = State::Ground;
                    }
                }
            }
        }
        writes
    }
}

fn is_clipboard_write(sequence: &[u8]) -> bool {
    let Some(body) = sequence.strip_prefix(b"\x1b]52;") else {
        return false;
    };
    let Some(separator) = body.iter().position(|byte| *byte == b';') else {
        return false;
    };
    !body[separator + 1..].starts_with(b"?")
}
