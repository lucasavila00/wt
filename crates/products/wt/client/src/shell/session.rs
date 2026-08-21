use super::model::ShellWorld;
use anyhow::{Context as _, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read as _, Write as _};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

const OUTPUT_QUEUE: usize = 256;
const SCROLLBACK_ROWS: usize = 10_000;

enum SessionEvent {
    Output { token: u64, bytes: Vec<u8> },
    Closed { token: u64, error: Option<String> },
}

pub(super) struct SessionSet {
    sessions: Vec<WorldSession>,
    events: Option<Receiver<SessionEvent>>,
    sender: SyncSender<SessionEvent>,
    next_token: u64,
}

impl SessionSet {
    pub(super) fn start(worlds: &[ShellWorld], rows: u16, columns: u16) -> Result<Self> {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let mut set = Self {
            sessions: Vec::with_capacity(worlds.len()),
            events: Some(events),
            sender: sender.clone(),
            next_token: 0,
        };
        for world in worlds {
            set.add_world(world, rows, columns)?;
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
                SessionEvent::Output { token, bytes } => {
                    let Some(index) = self.token_index(token) else {
                        continue;
                    };
                    let writes = self.sessions[index].clipboard.process(&bytes);
                    if index == active {
                        clipboard_writes.extend(writes);
                    }
                    self.sessions[index].parser.process(&bytes);
                }
                SessionEvent::Closed { token, error } => {
                    let Some(index) = self.token_index(token) else {
                        continue;
                    };
                    self.sessions[index].mark_closed(error);
                }
            }
        }
        (changed, clipboard_writes)
    }

    pub(super) fn screen(&self, index: usize) -> &vt100::Screen {
        self.sessions[index].parser.screen()
    }

    pub(super) fn closed_message(&self, index: usize) -> Option<&str> {
        self.sessions[index].closed_message.as_deref()
    }

    pub(super) fn restart(&mut self, index: usize, rows: u16, columns: u16) {
        let token = self.next_token;
        self.next_token = self
            .next_token
            .checked_add(1)
            .expect("shell session token overflow");
        let world = self.sessions[index].world.clone();
        match WorldSession::start_ssh(token, &world, rows, columns, &self.sender) {
            Ok(session) => self.sessions[index] = session,
            Err(error) => {
                self.sessions[index].closed_message =
                    Some(format!("SSH reconnect failed: {error:#}"));
            }
        }
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
            if let Some(master) = session.master.as_ref() {
                master
                    .resize(size)
                    .with_context(|| format!("resize {} terminal", session.name))?;
            }
            session.parser.set_size(rows, columns);
        }
        Ok(())
    }

    pub(super) fn add_world(&mut self, world: &ShellWorld, rows: u16, columns: u16) -> Result<()> {
        let token = self.next_token;
        self.next_token = self
            .next_token
            .checked_add(1)
            .expect("shell session token overflow");
        self.sessions.push(WorldSession::start_ssh(
            token,
            world,
            rows,
            columns,
            &self.sender,
        )?);
        Ok(())
    }

    pub(super) fn reconcile(
        &mut self,
        worlds: &[ShellWorld],
        rows: u16,
        columns: u16,
    ) -> Result<()> {
        self.sessions.retain_mut(|session| {
            let retained = worlds
                .iter()
                .any(|world| world.identity == session.world.identity);
            if !retained {
                session.stop_without_joining_reader();
            }
            retained
        });
        for world in worlds {
            if self.world_index(world).is_none() {
                self.add_world(world, rows, columns)?;
            }
        }
        self.sessions.sort_by_key(|session| {
            worlds
                .iter()
                .position(|world| world.identity == session.world.identity)
        });
        Ok(())
    }

    fn world_index(&self, world: &ShellWorld) -> Option<usize> {
        self.sessions
            .iter()
            .position(|session| session.world.identity == world.identity)
    }

    fn token_index(&self, token: u64) -> Option<usize> {
        self.sessions
            .iter()
            .position(|session| session.token == token)
    }
}

impl Drop for SessionSet {
    fn drop(&mut self) {
        self.events.take();
        self.sessions.clear();
    }
}

struct WorldSession {
    token: u64,
    world: ShellWorld,
    name: String,
    parser: vt100::Parser,
    writer: Option<Box<dyn std::io::Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    reader: Option<JoinHandle<()>>,
    clipboard: ClipboardRelay,
    closed_message: Option<String>,
}

impl WorldSession {
    fn start_ssh(
        token: u64,
        world: &ShellWorld,
        rows: u16,
        columns: u16,
        sender: &SyncSender<SessionEvent>,
    ) -> Result<Self> {
        let mut command = CommandBuilder::new("ssh");
        command.args(["--", &world.name]);
        Self::start(token, world, command, rows, columns, sender)
    }

    fn start(
        token: u64,
        world: &ShellWorld,
        command: CommandBuilder,
        rows: u16,
        columns: u16,
        sender: &SyncSender<SessionEvent>,
    ) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(pty_size(rows, columns))
            .with_context(|| format!("open {} terminal", world.name))?;
        let reader = pair
            .master
            .try_clone_reader()
            .with_context(|| format!("open {} terminal output", world.name))?;
        let writer = pair
            .master
            .take_writer()
            .with_context(|| format!("open {} terminal input", world.name))?;
        let child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("start SSH for {}", world.name))?;
        drop(pair.slave);
        let output_sender = sender.clone();
        let event_token = token;
        let reader = thread::Builder::new()
            .name(format!("wt-shell-{}", world.name))
            .spawn(move || read_output(event_token, reader, &output_sender))
            .with_context(|| format!("start {} terminal reader", world.name))?;
        Ok(Self {
            token,
            world: world.clone(),
            name: world.name.clone(),
            parser: vt100::Parser::new(rows, columns, SCROLLBACK_ROWS),
            writer: Some(writer),
            master: Some(pair.master),
            child: Some(child),
            reader: Some(reader),
            clipboard: ClipboardRelay::default(),
            closed_message: None,
        })
    }

    fn mark_closed(&mut self, reader_error: Option<String>) {
        let status = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok())
            .flatten();
        self.closed_message = Some(match (reader_error, status) {
            (Some(error), _) => format!("SSH session reader failed: {error}"),
            (None, Some(status)) if status.success() => "SSH session ended".into(),
            (None, Some(status)) => format!("SSH session ended: {status}"),
            (None, None) => "SSH connection closed".into(),
        });
        self.writer.take();
        self.master.take();
        self.reader.take();
    }

    fn stop_without_joining_reader(&mut self) {
        self.writer.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.master.take();
        self.reader.take();
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
    token: u64,
    mut reader: Box<dyn std::io::Read + Send>,
    sender: &SyncSender<SessionEvent>,
) {
    let mut buffer = vec![0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(SessionEvent::Closed { token, error: None });
                return;
            }
            Ok(length) => {
                if sender
                    .send(SessionEvent::Output {
                        token,
                        bytes: buffer[..length].to_vec(),
                    })
                    .is_err()
                {
                    return;
                }
            }
            Err(error) if is_pty_eof(&error) => {
                let _ = sender.send(SessionEvent::Closed { token, error: None });
                return;
            }
            Err(error) => {
                let _ = sender.send(SessionEvent::Closed {
                    token,
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
            next_token: 2,
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
            next_token: 2,
        };
        wait_for(&mut sessions, 0, "ready");
        wait_for(&mut sessions, 1, "ready");
        let sequence = b"\x1b]52;c;Y29weQ==\x1b\\";
        let one = sessions.sessions[0].token;
        let two = sessions.sessions[1].token;

        sender
            .send(SessionEvent::Output {
                token: two,
                bytes: sequence.to_vec(),
            })
            .unwrap();
        assert!(sessions.drain_output(0).1.is_empty());
        sender
            .send(SessionEvent::Output {
                token: one,
                bytes: sequence.to_vec(),
            })
            .unwrap();
        assert_eq!(sessions.drain_output(0).1, vec![sequence.to_vec()]);
    }

    fn fake_session(token: u64, name: &str, sender: &SyncSender<SessionEvent>) -> WorldSession {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", r"stty -echo; printf '\033[2J\033[Hready'; cat"]);
        WorldSession::start(token, &ShellWorld::from(name), command, 4, 20, sender).unwrap()
    }

    #[test]
    fn reconciliation_adds_removes_and_reorders_sessions() {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let mut sessions = SessionSet {
            sessions: vec![
                fake_session(0, "one", &sender),
                fake_session(1, "two", &sender),
            ],
            events: Some(events),
            sender,
            next_token: 2,
        };
        let two = sessions.sessions[1].world.clone();
        let replacement = ShellWorld::from("one");
        let replacement_identity = replacement.identity.clone();

        sessions.reconcile(&[two, replacement], 4, 20).unwrap();

        assert_eq!(
            sessions
                .sessions
                .iter()
                .map(|session| session.name.as_str())
                .collect::<Vec<_>>(),
            vec!["two", "one"]
        );
        assert_eq!(sessions.sessions[1].world.identity, replacement_identity);
    }

    #[test]
    fn reconciliation_preserves_a_closed_session_for_manual_reconnect() {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let mut sessions = SessionSet {
            sessions: vec![fake_session(0, "one", &sender)],
            events: Some(events),
            sender,
            next_token: 1,
        };
        let world = sessions.sessions[0].world.clone();
        sessions.sessions[0].closed_message = Some("SSH connection closed".into());

        sessions.reconcile(&[world], 4, 20).unwrap();

        assert_eq!(sessions.sessions[0].token, 0);
        assert!(sessions.sessions[0].closed_message.is_some());
    }

    #[test]
    fn child_exit_is_reported() {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let world = ShellWorld::from("one");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exit 23"]);
        let session = WorldSession::start(0, &world, command, 4, 20, &sender).unwrap();
        let mut sessions = SessionSet {
            sessions: vec![session],
            events: Some(events),
            sender,
            next_token: 1,
        };

        let deadline = Instant::now() + Duration::from_secs(2);
        while sessions.closed_message(0).is_none() && Instant::now() < deadline {
            sessions.drain_output(0);
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(
            sessions.closed_message(0),
            Some("SSH session ended: Exited with code 23")
        );
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
