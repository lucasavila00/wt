use super::{ClipboardRelay, SessionEvent, ShellWorld, SCROLLBACK_ROWS};
use anyhow::{Context as _, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::Read as _;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};

pub(super) struct WorldSession {
    pub(super) token: u64,
    pub(super) world: ShellWorld,
    pub(super) control_path: PathBuf,
    pub(super) name: String,
    pub(super) parser: vt100::Parser,
    pub(super) writer: Option<Box<dyn std::io::Write + Send>>,
    pub(super) master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    reader: Option<JoinHandle<()>>,
    pub(super) clipboard: ClipboardRelay,
    pub(super) closed_message: Option<String>,
}

impl WorldSession {
    pub(super) fn start_ssh(
        token: u64,
        world: &ShellWorld,
        control_path: PathBuf,
        rows: u16,
        columns: u16,
        sender: &SyncSender<SessionEvent>,
    ) -> Result<Self> {
        let mut command = CommandBuilder::new("ssh");
        command.args(["-M", "-S"]);
        command.arg(&control_path);
        command.args(["--", &world.name]);
        Self::start(token, world, control_path, command, rows, columns, sender)
    }

    pub(super) fn start(
        token: u64,
        world: &ShellWorld,
        control_path: PathBuf,
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
            control_path,
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

    pub(super) fn mark_closed(&mut self, reader_error: Option<String>) {
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

    pub(super) fn stop_without_joining_reader(&mut self) {
        self.writer.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.master.take();
        self.reader.take();
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

pub(super) fn pty_size(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}
