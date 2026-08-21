use anyhow::{Context as _, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read as _, Write as _};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

const OUTPUT_QUEUE: usize = 256;
const SCROLLBACK_ROWS: usize = 10_000;

enum SessionEvent {
    Output { index: usize, bytes: Vec<u8> },
    Closed { index: usize, error: Option<String> },
}

pub(super) struct SessionSet {
    sessions: Vec<WorldSession>,
    events: Option<Receiver<SessionEvent>>,
    sender: SyncSender<SessionEvent>,
}

impl SessionSet {
    pub(super) fn start(worlds: &[String], rows: u16, columns: u16) -> Result<Self> {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let mut set = Self {
            sessions: Vec::with_capacity(worlds.len()),
            events: Some(events),
            sender: sender.clone(),
        };
        for (index, world) in worlds.iter().enumerate() {
            set.sessions.push(WorldSession::start_ssh(
                index, world, rows, columns, &sender,
            )?);
        }
        Ok(set)
    }

    pub(super) fn drain_output(&mut self, active: usize) -> (bool, Vec<Vec<u8>>) {
        let mut changed = false;
        let mut clipboard_writes = Vec::new();
        let events = self.events.as_ref().expect("session event receiver exists");
        while let Ok(event) = events.try_recv() {
            changed = true;
            match event {
                SessionEvent::Output { index, bytes } => {
                    let writes = self.sessions[index].clipboard.process(&bytes);
                    if index == active {
                        clipboard_writes.extend(writes);
                    }
                    self.sessions[index].parser.process(&bytes);
                }
                SessionEvent::Closed { index, error } => {
                    self.sessions[index].closed = true;
                    if let Some(error) = error {
                        let message = format!("\r\nwt shell: session reader failed: {error}\r\n");
                        self.sessions[index].parser.process(message.as_bytes());
                    }
                }
            }
        }
        (changed, clipboard_writes)
    }

    pub(super) fn screen(&self, index: usize) -> &vt100::Screen {
        self.sessions[index].parser.screen()
    }

    pub(super) fn write(&mut self, index: usize, bytes: &[u8]) -> Result<()> {
        self.sessions[index]
            .writer
            .as_mut()
            .context("world terminal input is closed")?
            .write_all(bytes)
            .with_context(|| format!("write input to {}", self.sessions[index].name))?;
        self.sessions[index]
            .writer
            .as_mut()
            .expect("writer checked above")
            .flush()
            .with_context(|| format!("flush input to {}", self.sessions[index].name))
    }

    pub(super) fn resize(&mut self, rows: u16, columns: u16) -> Result<()> {
        let size = pty_size(rows, columns);
        for session in &mut self.sessions {
            session
                .master
                .as_ref()
                .context("world terminal is closed")?
                .resize(size)
                .with_context(|| format!("resize {} terminal", session.name))?;
            session.parser.set_size(rows, columns);
        }
        Ok(())
    }

    pub(super) fn all_closed(&self) -> bool {
        !self.sessions.is_empty() && self.sessions.iter().all(|session| session.closed)
    }

    pub(super) fn add_world(&mut self, world: &str, rows: u16, columns: u16) -> Result<()> {
        let index = self.sessions.len();
        self.sessions.push(WorldSession::start_ssh(
            index,
            world,
            rows,
            columns,
            &self.sender,
        )?);
        Ok(())
    }

    pub(super) fn is_open(&self, index: usize) -> bool {
        self.sessions
            .get(index)
            .is_some_and(|session| !session.closed)
    }
}

impl Drop for SessionSet {
    fn drop(&mut self) {
        self.events.take();
        self.sessions.clear();
    }
}

struct WorldSession {
    name: String,
    parser: vt100::Parser,
    writer: Option<Box<dyn std::io::Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    reader: Option<JoinHandle<()>>,
    clipboard: ClipboardRelay,
    closed: bool,
}

impl WorldSession {
    fn start_ssh(
        index: usize,
        world: &str,
        rows: u16,
        columns: u16,
        sender: &SyncSender<SessionEvent>,
    ) -> Result<Self> {
        let mut command = CommandBuilder::new("ssh");
        command.args(["--", world]);
        Self::start(index, world, command, rows, columns, sender)
    }

    fn start(
        index: usize,
        name: &str,
        command: CommandBuilder,
        rows: u16,
        columns: u16,
        sender: &SyncSender<SessionEvent>,
    ) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(pty_size(rows, columns))
            .with_context(|| format!("open {name} terminal"))?;
        let reader = pair
            .master
            .try_clone_reader()
            .with_context(|| format!("open {name} terminal output"))?;
        let writer = pair
            .master
            .take_writer()
            .with_context(|| format!("open {name} terminal input"))?;
        let child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("start SSH for {name}"))?;
        drop(pair.slave);
        let output_sender = sender.clone();
        let reader = thread::Builder::new()
            .name(format!("wt-shell-{index}"))
            .spawn(move || read_output(index, reader, &output_sender))
            .with_context(|| format!("start {name} terminal reader"))?;
        Ok(Self {
            name: name.to_owned(),
            parser: vt100::Parser::new(rows, columns, SCROLLBACK_ROWS),
            writer: Some(writer),
            master: Some(pair.master),
            child: Some(child),
            reader: Some(reader),
            clipboard: ClipboardRelay::default(),
            closed: false,
        })
    }
}

#[derive(Default)]
struct ClipboardRelay {
    sequence: Vec<u8>,
    state: ClipboardRelayState,
}

#[derive(Default)]
enum ClipboardRelayState {
    #[default]
    Ground,
    Escape,
    Osc,
}

impl ClipboardRelay {
    fn process(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut writes = Vec::new();
        for &byte in bytes {
            match self.state {
                ClipboardRelayState::Ground if byte == b'\x1b' => {
                    self.sequence.push(byte);
                    self.state = ClipboardRelayState::Escape;
                }
                ClipboardRelayState::Ground => {}
                ClipboardRelayState::Escape if byte == b']' => {
                    self.sequence.push(byte);
                    self.state = ClipboardRelayState::Osc;
                }
                ClipboardRelayState::Escape if byte == b'\x1b' => {
                    self.sequence.clear();
                    self.sequence.push(byte);
                }
                ClipboardRelayState::Escape => {
                    self.sequence.clear();
                    self.state = ClipboardRelayState::Ground;
                }
                ClipboardRelayState::Osc => {
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
                        self.state = ClipboardRelayState::Ground;
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

impl Drop for WorldSession {
    fn drop(&mut self) {
        self.writer.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.master.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_output(
    index: usize,
    mut reader: Box<dyn std::io::Read + Send>,
    sender: &SyncSender<SessionEvent>,
) {
    let mut buffer = vec![0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(SessionEvent::Closed { index, error: None });
                return;
            }
            Ok(length) => {
                if sender
                    .send(SessionEvent::Output {
                        index,
                        bytes: buffer[..length].to_vec(),
                    })
                    .is_err()
                {
                    return;
                }
            }
            Err(error) if is_pty_eof(&error) => {
                let _ = sender.send(SessionEvent::Closed { index, error: None });
                return;
            }
            Err(error) => {
                let _ = sender.send(SessionEvent::Closed {
                    index,
                    error: Some(error.to_string()),
                });
                return;
            }
        }
    }
}

fn is_pty_eof(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::UnexpectedEof
        || error.kind() == std::io::ErrorKind::BrokenPipe
        || error.raw_os_error() == Some(nix::errno::Errno::EIO as i32)
}

fn pty_size(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn hidden_sessions_keep_running_and_parsing_output() {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let mut sessions = SessionSet {
            sessions: vec![
                fake_session(0, "one", &sender),
                fake_session(1, "two", &sender),
            ],
            events: Some(events),
            sender,
        };
        wait_for(&mut sessions, 0, "ready");
        wait_for(&mut sessions, 1, "ready");

        sessions.write(1, b"hidden\r").unwrap();
        wait_for(&mut sessions, 1, "readyhidden");

        insta::assert_snapshot!(sessions.screen(0).contents(), @"ready");
        insta::assert_snapshot!(sessions.screen(1).contents(), @"readyhidden");
    }

    #[test]
    fn relays_clipboard_writes_only_from_the_active_session() {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let mut sessions = SessionSet {
            sessions: vec![
                fake_session(0, "one", &sender),
                fake_session(1, "two", &sender),
            ],
            events: Some(events),
            sender: sender.clone(),
        };
        wait_for(&mut sessions, 0, "ready");
        wait_for(&mut sessions, 1, "ready");
        let sequence = b"\x1b]52;c;Y29weQ==\x1b\\";

        sender
            .send(SessionEvent::Output {
                index: 1,
                bytes: sequence.to_vec(),
            })
            .unwrap();
        assert!(sessions.drain_output(0).1.is_empty());
        sender
            .send(SessionEvent::Output {
                index: 0,
                bytes: sequence.to_vec(),
            })
            .unwrap();
        assert_eq!(sessions.drain_output(0).1, vec![sequence.to_vec()]);
    }

    fn fake_session(index: usize, name: &str, sender: &SyncSender<SessionEvent>) -> WorldSession {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", r"stty -echo; printf '\033[2J\033[Hready'; cat"]);
        WorldSession::start(index, name, command, 4, 20, sender).unwrap()
    }

    fn wait_for(sessions: &mut SessionSet, index: usize, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            sessions.drain_output(index);
            if sessions.screen(index).contents() == expected {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "session did not render {expected:?}; rendered {:?}",
            sessions.screen(index).contents()
        );
    }

    #[test]
    fn relays_complete_osc52_writes_across_output_chunks() {
        let mut relay = ClipboardRelay::default();

        assert!(relay.process(b"before\x1b]52;c;Y29").is_empty());
        assert_eq!(
            relay.process(b"weQ==\x1b\\after"),
            vec![b"\x1b]52;c;Y29weQ==\x1b\\".to_vec()]
        );
        assert_eq!(
            relay.process(b"\x1b]52;c;Y29weQ==\x07"),
            vec![b"\x1b]52;c;Y29weQ==\x07".to_vec()]
        );
    }

    #[test]
    fn does_not_relay_other_osc_sequences_or_clipboard_reads() {
        let mut relay = ClipboardRelay::default();

        assert!(relay.process(b"\x1b]2;window title\x07").is_empty());
        assert!(relay.process(b"\x1b]52;c;?\x1b\\").is_empty());
    }
}
