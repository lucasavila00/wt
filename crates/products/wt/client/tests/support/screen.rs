use anyhow::{bail, Context as _, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const ROWS: u16 = 30;
const COLUMNS: u16 = 100;
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum Key {
    Char(char),
    Tab,
    BackTab,
    Backspace,
    Enter,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Function(u8),
    ShiftFunction(u8),
    Ctrl(char),
}

pub struct Screen {
    parser: vt100::Parser,
    output: Receiver<Vec<u8>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn Child + Send + Sync>,
    timeout: Duration,
}

impl Screen {
    pub fn launch(
        binary: impl AsRef<Path>,
        arguments: &[&str],
        cwd: impl AsRef<Path>,
        environment: &[(&str, OsString)],
        timeout: Duration,
    ) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLUMNS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open wt test PTY")?;
        let reader = pair
            .master
            .try_clone_reader()
            .context("clone wt PTY reader")?;
        let writer = pair.master.take_writer().context("open wt PTY writer")?;
        let mut command = CommandBuilder::new(binary.as_ref().as_os_str());
        command.args(arguments.iter().map(OsStr::new));
        command.cwd(cwd.as_ref().as_os_str());
        command.env("TERM", "xterm-256color");
        for (key, value) in environment {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .context("launch wt in test PTY")?;
        drop(pair.slave);
        let (sender, output) = mpsc::sync_channel(64);
        thread::spawn(move || read_output(reader, &sender));
        Ok(Self {
            parser: vt100::Parser::new(ROWS, COLUMNS, 0),
            output,
            writer: Some(writer),
            child,
            timeout,
        })
    }

    pub fn press(&mut self, key: Key) -> Result<&mut Self> {
        self.write(&key_bytes(key)?)?;
        Ok(self)
    }

    pub fn type_text(&mut self, text: &str) -> Result<&mut Self> {
        self.write(text.as_bytes())?;
        self.wait_for_text(text)
    }

    #[allow(dead_code)]
    pub fn click(&mut self, column: u16, row: u16) -> Result<&mut Self> {
        self.write(
            format!(
                "\x1b[<0;{};{}M",
                column.saturating_add(1),
                row.saturating_add(1)
            )
            .as_bytes(),
        )?;
        Ok(self)
    }

    pub fn wait_for_text(&mut self, text: &str) -> Result<&mut Self> {
        let deadline = Instant::now() + self.timeout;
        loop {
            self.pump_available();
            if self.contents().contains(text) {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "text {text:?} was not visible within {:?}\n{}",
                    self.timeout,
                    self.contents()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    #[allow(dead_code)]
    pub fn wait_for_text_gone(&mut self, text: &str) -> Result<&mut Self> {
        let deadline = Instant::now() + self.timeout;
        loop {
            self.pump_available();
            if !self.contents().contains(text) {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "text {text:?} remained visible for {:?}\n{}",
                    self.timeout,
                    self.contents()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    #[allow(dead_code)]
    pub fn wait_for_exit(&mut self, expected_code: u32) -> Result<&mut Self> {
        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(status) = self.child.try_wait().context("poll wt process")? {
                if status.exit_code() != expected_code {
                    bail!(
                        "wt exited with code {}, expected {expected_code}\n{}",
                        status.exit_code(),
                        self.contents()
                    );
                }
                while let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(50)) {
                    self.parser.process(&bytes);
                }
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "wt did not exit within {:?}\n{}",
                    self.timeout,
                    self.contents()
                );
            }
            self.pump_available();
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn contents(&self) -> String {
        self.parser.screen().contents()
    }

    #[allow(dead_code)]
    pub fn wait_for_quiet(&mut self, interval: Duration) -> Result<&mut Self> {
        let deadline = Instant::now() + self.timeout;
        let mut quiet_until = Instant::now() + interval;
        loop {
            let now = Instant::now();
            if now >= quiet_until {
                return Ok(self);
            }
            if now >= deadline {
                bail!(
                    "wt terminal output did not become quiet within {:?}",
                    self.timeout
                );
            }
            match self
                .output
                .recv_timeout(quiet_until.saturating_duration_since(now))
            {
                Ok(bytes) => {
                    self.parser.process(&bytes);
                    quiet_until = Instant::now() + interval;
                }
                Err(RecvTimeoutError::Timeout) => return Ok(self),
                Err(RecvTimeoutError::Disconnected) => bail!("wt PTY output stopped"),
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let writer = self.writer.as_mut().context("wt test PTY is closed")?;
        writer.write_all(bytes).context("write wt test input")?;
        writer.flush().context("flush wt test input")
    }

    fn pump_available(&mut self) {
        while let Ok(bytes) = self.output.try_recv() {
            self.parser.process(&bytes);
        }
    }

    fn pump_until(&mut self, deadline: Instant) -> Result<()> {
        let wait = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(50));
        match self.output.recv_timeout(wait) {
            Ok(bytes) => self.parser.process(&bytes),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(status) = self.child.try_wait().context("poll wt process")? {
                    self.pump_available();
                    bail!(
                        "wt exited before the expected UI appeared: {status:?}\n{}",
                        self.contents()
                    )
                }
                bail!("wt PTY output stopped")
            }
        }
        Ok(())
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.writer.take();
    }
}

fn read_output(mut reader: Box<dyn Read + Send>, output: &mpsc::SyncSender<Vec<u8>>) {
    let mut buffer = [0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(length) if output.send(buffer[..length].to_vec()).is_err() => return,
            Ok(_) => {}
        }
    }
}

fn key_bytes(key: Key) -> Result<Vec<u8>> {
    Ok(match key {
        Key::Char(character) => character.to_string().into_bytes(),
        Key::Tab => b"\t".to_vec(),
        Key::BackTab => b"\x1b[Z".to_vec(),
        Key::Backspace => b"\x7f".to_vec(),
        Key::Enter => b"\r".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        Key::Up => b"\x1b[A".to_vec(),
        Key::Down => b"\x1b[B".to_vec(),
        Key::Right => b"\x1b[C".to_vec(),
        Key::Left => b"\x1b[D".to_vec(),
        Key::Function(1) => b"\x1bOP".to_vec(),
        Key::Function(5) => b"\x1b[15~".to_vec(),
        Key::Function(6) => b"\x1b[17~".to_vec(),
        Key::Function(number) => bail!("unsupported function key F{number}"),
        Key::ShiftFunction(5) => b"\x1b[15;2~".to_vec(),
        Key::ShiftFunction(number) => bail!("unsupported shifted function key F{number}"),
        Key::Ctrl(character) if character.is_ascii_alphabetic() => {
            vec![(character.to_ascii_lowercase() as u8) & 0x1f]
        }
        Key::Ctrl(character) => bail!("unsupported control key {character:?}"),
    })
}
