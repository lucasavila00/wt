use crate::install;
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);
const STDERR_LIMIT: usize = 64 * 1024;

pub(crate) fn reconcile() -> Result<()> {
    let codex = install::real_codex()?;
    reconcile_with_codex(&codex)
}

pub(crate) fn reconcile_with_codex(codex: &Path) -> Result<()> {
    let home = codex_home()?;
    reconcile_home(codex, &home)
}

fn reconcile_home(codex: &Path, home: &Path) -> Result<()> {
    let mut server = AppServer::start(codex, home, TIMEOUT)?;
    let initialized: InitializeResult = server.call(
        "initialize",
        json!({
            "clientInfo": {"name": "wt-codex-integration", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"experimentalApi": true}
        }),
    )?;
    server.notify("initialized")?;
    if !same_path(Path::new(&initialized.codex_home), home) {
        bail!(
            "Codex app-server opened {}, expected {}",
            initialized.codex_home,
            home.display()
        );
    }

    // Codex documents the default thread/list behavior as scanning rollout
    // files and repairing the state database before returning picker results.
    let _: Value = server.call("thread/list", json!({"limit": 1}))?;
    server.close();

    Ok(())
}

fn codex_home() -> Result<PathBuf> {
    match env::var_os("CODEX_HOME") {
        Some(home) if !home.is_empty() => Ok(PathBuf::from(home)),
        _ => env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".codex"))
            .context("neither CODEX_HOME nor HOME is set"),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    codex_home: String,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Value,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.data.is_null() {
            write!(formatter, "Codex RPC {}: {}", self.code, self.message)
        } else {
            write!(
                formatter,
                "Codex RPC {}: {} ({})",
                self.code, self.message, self.data
            )
        }
    }
}

impl std::error::Error for RpcError {}

#[derive(Deserialize)]
struct RpcResponse {
    id: Option<u64>,
    #[serde(default)]
    result: Value,
    error: Option<RpcError>,
}

enum ServerOutput {
    Line(String),
    Error(std::io::Error),
    Eof,
}

struct AppServer {
    child: Child,
    stdin: Option<ChildStdin>,
    output: mpsc::Receiver<ServerOutput>,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_complete: mpsc::Receiver<()>,
    deadline: Instant,
    next_id: u64,
}

impl AppServer {
    fn start(codex: &Path, home: &Path, timeout: Duration) -> Result<Self> {
        let mut child = Command::new(codex)
            .args(["app-server", "--stdio"])
            .env("CODEX_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("start {} app-server", codex.display()))?;
        let stdin = child.stdin.take().context("open Codex app-server stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("open Codex app-server stdout")?;
        let stderr_pipe = child
            .stderr
            .take()
            .context("open Codex app-server stderr")?;

        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if sender.send(ServerOutput::Line(line)).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(ServerOutput::Error(error));
                        return;
                    }
                }
            }
            let _ = sender.send(ServerOutput::Eof);
        });

        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stderr_tail = Arc::clone(&stderr);
        let (stderr_complete_sender, stderr_complete) = mpsc::channel();
        thread::spawn(move || {
            drain_stderr(stderr_pipe, stderr_tail);
            let _ = stderr_complete_sender.send(());
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            output,
            stderr,
            stderr_complete,
            deadline: Instant::now() + timeout,
            next_id: 0,
        })
    }

    fn call<T: DeserializeOwned>(&mut self, method: &str, params: Value) -> Result<T> {
        self.next_id += 1;
        let id = self.next_id;
        if let Err(error) =
            self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
        {
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == ErrorKind::BrokenPipe)
            {
                return Err(self.stopped_before_reply(method));
            }
            return Err(error);
        }
        loop {
            let remaining = self
                .deadline
                .checked_duration_since(Instant::now())
                .context("Codex app-server timed out")?;
            match self.output.recv_timeout(remaining) {
                Ok(ServerOutput::Line(line)) => {
                    let Ok(response) = serde_json::from_str::<RpcResponse>(&line) else {
                        continue;
                    };
                    if response.id != Some(id) {
                        continue;
                    }
                    if let Some(error) = response.error {
                        return Err(error.into());
                    }
                    return serde_json::from_value(response.result)
                        .with_context(|| format!("decode Codex {method} response"));
                }
                Ok(ServerOutput::Error(error)) => return Err(error.into()),
                Ok(ServerOutput::Eof) => return Err(self.stopped_before_reply(method)),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    bail!(
                        "Codex app-server timed out during {method}{}",
                        self.stderr_suffix()
                    )
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!(
                        "Codex app-server output closed during {method}{}",
                        self.stderr_suffix()
                    )
                }
            }
        }
    }

    fn notify(&mut self, method: &str) -> Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "method": method}))
    }

    fn write(&mut self, message: &Value) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("Codex app-server stdin is closed")?;
        let mut message = serde_json::to_vec(message)?;
        message.push(b'\n');
        stdin.write_all(&message)?;
        stdin.flush()?;
        Ok(())
    }

    fn stopped_before_reply(&mut self, method: &str) -> anyhow::Error {
        if self.child.try_wait().ok().flatten().is_some() {
            let _ = self
                .stderr_complete
                .recv_timeout(Duration::from_millis(100));
        }
        anyhow::anyhow!(
            "Codex app-server stopped before {method} replied{}",
            self.stderr_suffix()
        )
    }

    fn stderr_suffix(&self) -> String {
        let text = String::from_utf8_lossy(&self.stderr.lock().unwrap())
            .trim()
            .to_owned();
        if text.is_empty() {
            String::new()
        } else {
            format!(": {text}")
        }
    }

    fn close(&mut self) {
        self.stdin.take();
        for _ in 0..20 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        self.close();
    }
}

fn drain_stderr(mut pipe: impl Read, tail: Arc<Mutex<Vec<u8>>>) {
    let mut buffer = [0; 4096];
    while let Ok(count) = pipe.read(&mut buffer) {
        if count == 0 {
            return;
        }
        let mut tail = tail.lock().unwrap();
        tail.extend_from_slice(&buffer[..count]);
        if tail.len() > STDERR_LIMIT {
            let remove = tail.len() - STDERR_LIMIT;
            tail.drain(..remove);
        }
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf())
        == fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    const ID: &str = "33333333-3333-4333-8333-333333333333";

    fn write_rollout(home: &Path) {
        let directory = home.join("sessions/2026/08/20");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("rollout-2026-08-20T10-00-00-{ID}.jsonl")),
            format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{ID}\"}}}}\n"),
        )
        .unwrap();
    }

    fn fake_codex(path: &Path, home: &Path) {
        let script = format!(
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"codexHome":"{}"}}}}'
      ;;
    *'"method":"thread/list"'*)
      case "$line" in *useStateDbOnly*) exit 42 ;; esac
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"data":[{{"id":"{}"}}],"nextCursor":null}}}}\n' "$id"
      ;;
  esac
done
"#,
            home.display(),
            ID
        );
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn asks_codex_to_scan_and_repair_the_session_index() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        write_rollout(&home);
        let codex = temp.path().join("codex");
        fake_codex(&codex, &home);

        reconcile_home(&codex, &home).unwrap();
    }

    #[test]
    #[ignore = "requires an installed Codex CLI"]
    fn installed_codex_indexes_a_synthetic_rollout() {
        const LIVE_ID: &str = "01a021ff-ffff-7fff-8fff-ffffffffffff";
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        let directory = home.join("sessions/2026/08/20");
        fs::create_dir_all(&directory).unwrap();
        let rollout = directory.join(format!("rollout-2026-08-20T10-00-00-{LIVE_ID}.jsonl"));
        fs::write(
            rollout,
            format!(
                concat!(
                    "{{\"timestamp\":\"2026-08-20T10:00:00Z\",\"type\":\"session_meta\",",
                    "\"payload\":{{\"id\":\"{}\",\"timestamp\":\"2026-08-20T10:00:00Z\",",
                    "\"cwd\":\"{}\",\"originator\":\"codex-tui\",\"cli_version\":\"0.149.0\",",
                    "\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":null}}}}\n",
                    "{{\"timestamp\":\"2026-08-20T10:00:01Z\",\"type\":\"event_msg\",",
                    "\"payload\":{{\"type\":\"user_message\",\"message\":\"synthetic reconciliation\"}}}}\n",
                    "{{\"timestamp\":\"2026-08-20T10:00:01Z\",\"type\":\"response_item\",",
                    "\"payload\":{{\"type\":\"message\",\"role\":\"user\",",
                    "\"content\":[{{\"type\":\"input_text\",\"text\":\"synthetic reconciliation\"}}]}}}}\n"
                ),
                LIVE_ID,
                home.display()
            ),
        )
        .unwrap();

        reconcile_home(Path::new("codex"), &home).unwrap();
        reconcile_home(Path::new("codex"), &home).unwrap();
    }
}
