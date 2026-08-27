use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const RECONCILIATION_TIMEOUT: Duration = Duration::from_secs(270);

pub(crate) fn reconcile_with_codex(codex: &Path) -> Result<()> {
    let home = codex_home()?;
    reconcile_home(codex, &home)
}

fn reconcile_home(codex: &Path, home: &Path) -> Result<()> {
    reconcile_home_with_timeouts(codex, home, RESPONSE_TIMEOUT, RECONCILIATION_TIMEOUT)
}

fn reconcile_home_with_timeouts(
    codex: &Path,
    home: &Path,
    response_timeout: Duration,
    reconciliation_timeout: Duration,
) -> Result<()> {
    let mut server = AppServer::start(
        codex,
        home,
        response_timeout,
        Instant::now() + reconciliation_timeout,
    )?;
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

    for archived in [false, true] {
        let discovered = server.list_ids(false, archived)?;
        let verified = server.list_ids(true, archived)?;
        let mut absent = discovered
            .iter()
            .filter(|id| !verified.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        absent.sort_unstable();
        if !absent.is_empty() {
            let kind = if archived { "archived" } else { "active" };
            bail!(
                "Codex did not index these {kind} sessions: {}",
                absent.join(", ")
            );
        }
    }
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
    response_timeout: Duration,
    deadline: Instant,
    next_id: u64,
}

impl AppServer {
    fn start(
        codex: &Path,
        home: &Path,
        response_timeout: Duration,
        deadline: Instant,
    ) -> Result<Self> {
        let mut child = loop {
            match Command::new(codex)
                .args(["app-server", "--stdio"])
                .env("CODEX_HOME", home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => break child,
                Err(error)
                    if error.raw_os_error() == Some(nix::errno::Errno::ETXTBSY as i32)
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(16));
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("start {} app-server", codex.display()));
                }
            }
        };
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
            response_timeout,
            deadline,
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
        let response_deadline = Instant::now() + self.response_timeout;
        loop {
            let timeout = response_deadline
                .saturating_duration_since(Instant::now())
                .min(self.deadline.saturating_duration_since(Instant::now()));
            if timeout.is_zero() {
                return Err(self.timeout_error(method));
            }
            match self.output.recv_timeout(timeout) {
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
                    return Err(self.timeout_error(method));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(self.failure_with_stderr(format!(
                        "Codex app-server output closed during {method}"
                    )));
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

    fn list_ids(&mut self, state_only: bool, archived: bool) -> Result<HashSet<String>> {
        let mut ids = HashSet::new();
        let mut cursor = None;
        let mut cursors = HashSet::new();
        for _ in 0..10_000 {
            let mut params = json!({
                "archived": archived,
                "limit": 100,
                "useStateDbOnly": state_only,
            });
            if let Some(value) = cursor.take() {
                params["cursor"] = value;
            }
            let page: ThreadPage = self.call("thread/list", params)?;
            ids.extend(page.data.into_iter().map(|thread| thread.id));
            let Some(next) = page.next_cursor else {
                return Ok(ids);
            };
            let key = next.to_string();
            if !cursors.insert(key) {
                bail!("Codex thread/list repeated a pagination cursor");
            }
            cursor = Some(next);
        }
        bail!("Codex thread/list exceeded its pagination limit")
    }

    fn stopped_before_reply(&mut self, method: &str) -> anyhow::Error {
        self.failure_with_stderr(format!("Codex app-server stopped before {method} replied"))
    }

    fn timeout_error(&mut self, method: &str) -> anyhow::Error {
        let message = if Instant::now() >= self.deadline {
            format!("Codex session reconciliation exceeded its deadline during {method}")
        } else {
            format!(
                "Codex app-server did not reply to {method} within {:?}",
                self.response_timeout
            )
        };
        self.failure_with_stderr(message)
    }

    fn failure_with_stderr(&mut self, message: String) -> anyhow::Error {
        self.close();
        let _ = self.stderr_complete.recv_timeout(Duration::from_secs(1));
        anyhow::anyhow!("{message}{}", self.stderr_suffix())
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadPage {
    data: Vec<ThreadSummary>,
    next_cursor: Option<Value>,
}

#[derive(Deserialize)]
struct ThreadSummary {
    id: String,
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

    fn fake_codex(path: &Path, home: &Path, requests: &Path, mismatch: bool) {
        let script = format!(
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  printf '%s\n' "$line" >> '{}'
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"codexHome":"{}"}}}}'
      ;;
    *'"method":"thread/list"'*)
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      case "$line" in *'"archived":false'*) active=true ;; *) active=false ;; esac
      case "$line" in *'"useStateDbOnly":true'*) state_only=true ;; *) state_only=false ;; esac
      if test "$active" = true && {{ test "$state_only" = false || test "{}" = false; }}; then
        printf '{{"jsonrpc":"2.0","id":%s,"result":{{"data":[{{"id":"{}"}}],"nextCursor":null}}}}\n' "$id"
      else
        printf '{{"jsonrpc":"2.0","id":%s,"result":{{"data":[],"nextCursor":null}}}}\n' "$id"
      fi
      ;;
  esac
done
"#,
            requests.display(),
            home.display(),
            mismatch,
            ID
        );
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn fake_unresponsive_codex(path: &Path, home: &Path) {
        let script = format!(
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"codexHome":"{}"}}}}'
      ;;
    *'"method":"thread/list"'*) exec sleep 60 ;;
  esac
done
"#,
            home.display()
        );
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn fake_chatty_unresponsive_codex(path: &Path, home: &Path) {
        let script = format!(
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"codexHome":"{}"}}}}'
      ;;
    *'"method":"thread/list"'*)
      while :; do printf '%s\n' '{{"jsonrpc":"2.0","method":"codex/event"}}'; done
      ;;
  esac
done
"#,
            home.display()
        );
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn start_initialized_server(codex: &Path, home: &Path) -> AppServer {
        let mut server = AppServer::start(
            codex,
            home,
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        let _: InitializeResult = server
            .call(
                "initialize",
                json!({
                    "clientInfo": {"name": "wt-codex-integration", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": {"experimentalApi": true}
                }),
            )
            .unwrap();
        server.notify("initialized").unwrap();
        server
    }

    #[test]
    fn asks_codex_to_scan_and_verify_active_and_archived_threads() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        let codex = temp.path().join("codex");
        let requests = temp.path().join("requests.jsonl");
        fake_codex(&codex, &home, &requests, false);

        reconcile_home(&codex, &home).unwrap();
        let requests = fs::read_to_string(requests)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        let listings = requests
            .iter()
            .filter(|request| request["method"] == "thread/list")
            .map(|request| request["params"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            listings,
            vec![
                json!({"archived": false, "limit": 100, "useStateDbOnly": false}),
                json!({"archived": false, "limit": 100, "useStateDbOnly": true}),
                json!({"archived": true, "limit": 100, "useStateDbOnly": false}),
                json!({"archived": true, "limit": 100, "useStateDbOnly": true}),
            ]
        );
        let initialize = requests
            .iter()
            .find(|request| request["method"] == "initialize")
            .unwrap();
        assert_eq!(
            initialize["params"]["capabilities"],
            json!({"experimentalApi": true})
        );
        assert!(requests
            .iter()
            .all(|request| request["method"] != "thread/read"));
    }

    #[test]
    fn fails_when_codex_does_not_persist_a_scanned_thread() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        let codex = temp.path().join("codex");
        fake_codex(&codex, &home, &temp.path().join("requests.jsonl"), true);

        insta::assert_snapshot!(
            reconcile_home(&codex, &home).unwrap_err().to_string(),
            @"Codex did not index these active sessions: 33333333-3333-4333-8333-333333333333"
        );
    }

    #[test]
    fn fails_when_codex_app_server_stops_replying() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        let codex = temp.path().join("codex");
        fake_unresponsive_codex(&codex, &home);
        let mut server = start_initialized_server(&codex, &home);
        server.response_timeout = Duration::from_millis(10);

        insta::assert_snapshot!(
            server.list_ids(false, false).unwrap_err().to_string(),
            @"Codex app-server did not reply to thread/list within 10ms"
        );
    }

    #[test]
    fn fails_when_codex_app_server_emits_unrelated_output() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        let codex = temp.path().join("codex");
        fake_chatty_unresponsive_codex(&codex, &home);
        let mut server = start_initialized_server(&codex, &home);
        server.response_timeout = Duration::from_millis(10);

        insta::assert_snapshot!(
            server.list_ids(false, false).unwrap_err().to_string(),
            @"Codex app-server did not reply to thread/list within 10ms"
        );
    }

    #[test]
    fn fails_within_the_reconciliation_deadline() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        let codex = temp.path().join("codex");
        fake_unresponsive_codex(&codex, &home);
        let mut server = start_initialized_server(&codex, &home);
        server.deadline = Instant::now();

        insta::assert_snapshot!(
            server.list_ids(false, false).unwrap_err().to_string(),
            @"Codex session reconciliation exceeded its deadline during thread/list"
        );
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
                    "\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"UserMessage\",",
                    "\"content\":[{{\"type\":\"text\",\"text\":\"synthetic reconciliation\"}}]}}}}}}\n"
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
