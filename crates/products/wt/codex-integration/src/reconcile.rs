use crate::install;
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

const MAX_SESSION_META_BYTES: u64 = 64 * 1024;

pub(crate) fn reconcile() -> Result<()> {
    let codex = install::real_codex()?;
    reconcile_with_codex(&codex)
}

pub(crate) fn reconcile_with_codex(codex: &Path) -> Result<()> {
    let home = codex_home()?;
    reconcile_home(codex, &home)
}

fn reconcile_home(codex: &Path, home: &Path) -> Result<()> {
    let ids = rollout_ids(&home.join("sessions"))?;
    if ids.is_empty() {
        return Ok(());
    }

    let mut server = AppServer::start(codex, home)?;
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

    let mut state_only = true;
    let discovered = match server.list_ids(true) {
        Ok(ids) => ids,
        Err(error)
            if error
                .downcast_ref::<RpcError>()
                .is_some_and(RpcError::is_invalid_params) =>
        {
            state_only = false;
            server.list_ids(false)?
        }
        Err(error) => return Err(error),
    };
    let missing = ids
        .iter()
        .filter(|id| !discovered.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in &missing {
        let _: Value = server.call(
            "thread/read",
            json!({"threadId": id, "includeTurns": false}),
        )?;
    }

    let verified = server.list_ids(state_only)?;
    let absent = ids
        .iter()
        .filter(|id| !verified.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    if !absent.is_empty() {
        bail!("Codex did not index these sessions: {}", absent.join(", "));
    }
    server.close();

    Ok(())
}

fn rollout_ids(root: &Path) -> Result<BTreeSet<String>> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    let mut files = Vec::new();
    collect_rollouts(root, &mut files)?;
    files.sort();

    let mut ids = BTreeSet::new();
    for path in files {
        if let Some(id) = rollout_id(&path)
            .with_context(|| format!("inspect shared rollout {}", path.display()))?
        {
            ids.insert(id);
        }
    }
    Ok(ids)
}

fn collect_rollouts(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read session directory {}", directory.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_rollouts(&entry.path(), files)?;
        } else if file_type.is_file()
            && entry.file_name().to_string_lossy().starts_with("rollout-")
            && entry
                .path()
                .extension()
                .is_some_and(|value| value == "jsonl")
        {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn rollout_id(path: &Path) -> Result<Option<String>> {
    let file = File::open(path).with_context(|| format!("open rollout {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader
        .by_ref()
        .take(MAX_SESSION_META_BYTES + 1)
        .read_line(&mut line)?;
    if line.is_empty() {
        bail!("rollout is empty");
    }
    if line.len() as u64 > MAX_SESSION_META_BYTES {
        bail!("first rollout record is too large");
    }
    let record: SessionRecord =
        serde_json::from_str(&line).context("first rollout record is not valid JSON")?;
    if record.kind != "session_meta" {
        bail!("first rollout record is not session_meta");
    }
    if record.payload.is_subagent() {
        return Ok(None);
    }
    let id = record.payload.id.context("session_meta has no thread ID")?;
    let parsed = uuid::Uuid::parse_str(&id).context("session_meta thread ID is not a UUID")?;
    if id != parsed.hyphenated().to_string() {
        bail!("session_meta thread ID is not canonical");
    }
    for record in serde_json::Deserializer::from_reader(reader).into_iter::<ConversationRecord>() {
        let record = record.context("rollout record is not valid JSON")?;
        if record.kind == "event_msg" && record.payload.kind.as_deref() == Some("user_message") {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

#[derive(Deserialize)]
struct SessionRecord {
    #[serde(rename = "type")]
    kind: String,
    payload: SessionPayload,
}

#[derive(Deserialize)]
struct SessionPayload {
    id: Option<String>,
    #[serde(default)]
    source: Value,
}

#[derive(Deserialize)]
struct ConversationRecord {
    #[serde(rename = "type")]
    kind: String,
    payload: ConversationPayload,
}

#[derive(Deserialize)]
struct ConversationPayload {
    #[serde(rename = "type")]
    kind: Option<String>,
}

impl SessionPayload {
    fn is_subagent(&self) -> bool {
        self.source.as_object().is_some_and(|source| {
            source.contains_key("subagent") || source.contains_key("subAgent")
        })
    }
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

impl RpcError {
    fn is_invalid_params(&self) -> bool {
        self.code == -32602
    }
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
    next_id: u64,
}

impl AppServer {
    fn start(codex: &Path, home: &Path) -> Result<Self> {
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
            match self.output.recv() {
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
                Err(mpsc::RecvError) => {
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

    fn list_ids(&mut self, state_only: bool) -> Result<HashSet<String>> {
        let mut ids = HashSet::new();
        let mut cursor = None;
        let mut cursors = HashSet::new();
        for _ in 0..10_000 {
            let mut params = json!({"limit": 100});
            if state_only {
                params["useStateDbOnly"] = Value::Bool(true);
            }
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

    fn write_rollout(home: &Path) {
        let directory = home.join("sessions/2026/08/20");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("rollout-2026-08-20T10-00-00-{ID}.jsonl")),
            format!(
                concat!(
                    "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\"}}}}\n",
                    "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\"}}}}\n"
                ),
                ID
            ),
        )
        .unwrap();
    }

    fn fake_codex(path: &Path, home: &Path) {
        let script = format!(
            r#"#!/bin/sh
set -eu
indexed=false
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"codexHome":"{}"}}}}'
      ;;
    *'"method":"thread/list"'*)
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      if "$indexed"; then
        printf '{{"jsonrpc":"2.0","id":%s,"result":{{"data":[{{"id":"{}"}}],"nextCursor":null}}}}\n' "$id"
      else
        printf '{{"jsonrpc":"2.0","id":%s,"result":{{"data":[],"nextCursor":null}}}}\n' "$id"
      fi
      ;;
    *'"method":"thread/read"'*)
      id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
      indexed=true
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"thread":{{}}}}}}\n' "$id"
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
    fn repairs_and_verifies_each_shared_session_before_returning() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        write_rollout(&home);
        let codex = temp.path().join("codex");
        fake_codex(&codex, &home);

        reconcile_home(&codex, &home).unwrap();
    }

    #[test]
    fn rollout_scan_skips_sessions_without_a_user_prompt() {
        let temp = tempdir().unwrap();
        let sessions = temp.path().join("sessions/2026/08/20");
        fs::create_dir_all(&sessions).unwrap();
        let metadata_only = "11111111-1111-4111-8111-111111111111";
        let shell_only = "22222222-2222-4222-8222-222222222222";
        fs::write(
            sessions.join(format!("rollout-metadata-{metadata_only}.jsonl")),
            format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{metadata_only}\"}}}}\n"),
        )
        .unwrap();
        fs::write(
            sessions.join(format!("rollout-shell-{shell_only}.jsonl")),
            format!(
                concat!(
                    "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\"}}}}\n",
                    "{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",",
                    "\"role\":\"user\",\"content\":[]}}}}\n"
                ),
                shell_only
            ),
        )
        .unwrap();
        write_rollout(temp.path());

        assert_eq!(
            rollout_ids(&temp.path().join("sessions")).unwrap(),
            [ID.into()].into()
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
