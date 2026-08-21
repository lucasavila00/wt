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
    events: Receiver<SessionEvent>,
}

impl SessionSet {
    pub(super) fn start(worlds: &[String], rows: u16, columns: u16) -> Result<Self> {
        let (sender, events) = mpsc::sync_channel(OUTPUT_QUEUE);
        let sessions = worlds
            .iter()
            .enumerate()
            .map(|(index, world)| WorldSession::start_ssh(index, world, rows, columns, &sender))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { sessions, events })
    }

    pub(super) fn drain_output(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.events.try_recv() {
            changed = true;
            match event {
                SessionEvent::Output { index, bytes } => {
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
        changed
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
        self.sessions.iter().all(|session| session.closed)
    }
}

struct WorldSession {
    name: String,
    parser: vt100::Parser,
    writer: Option<Box<dyn std::io::Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    reader: Option<JoinHandle<()>>,
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
            closed: false,
        })
    }
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

fn pty_size(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}
