use crate::install;
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(20);
const STDERR_LIMIT: usize = 64 * 1024;

pub(crate) struct ReconcileResult {
    pub(crate) already_indexed: usize,
    pub(crate) reconciled: usize,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn reconcile() -> Result<ReconcileResult> {
    let codex = install::real_codex_in_path()?;
    reconcile_with_codex(&codex)
}

pub(crate) fn reconcile_with_codex(codex: &Path) -> Result<ReconcileResult> {
    let home = codex_home()?;
    reconcile_home(codex, &home)
}

fn reconcile_home(codex: &Path, home: &Path) -> Result<ReconcileResult> {
    let (ids, warnings) = rollout_ids(&home.join("sessions"))?;
    if ids.is_empty() {
        return Ok(ReconcileResult {
            already_indexed: 0,
            reconciled: 0,
            warnings,
        });
    }

    let mut server = AppServer::start(codex, home, TIMEOUT)?;
    let initialized: InitializeResult = server.call(
        "initialize",
        json!({
            "clientInfo": {"name": "wt-codex", "version": env!("CARGO_PKG_VERSION")},
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
    let already_indexed = ids.iter().filter(|id| discovered.contains(*id)).count();
    let missing = ids
        .iter()
        .filter(|id| !discovered.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    let mut picker_visible = BTreeSet::new();
    for id in &missing {
        let read: ThreadReadResult = server.call(
            "thread/read",
            json!({"threadId": id, "includeTurns": false}),
        )?;
        if !read.thread.preview.is_empty() {
            picker_visible.insert(id.clone());
        }
    }

    let verified = server.list_ids(state_only)?;
    let absent = picker_visible
        .iter()
        .filter(|id| !verified.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    if !absent.is_empty() {
        bail!("Codex did not index these sessions: {}", absent.join(", "));
    }
    server.close();

    Ok(ReconcileResult {
        already_indexed,
        reconciled: missing.iter().filter(|id| verified.contains(*id)).count(),
        warnings,
    })
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

fn rollout_ids(root: &Path) -> Result<(BTreeSet<String>, Vec<String>)> {
    if !root.exists() {
        return Ok((BTreeSet::new(), Vec::new()));
    }
    let mut files = Vec::new();
    collect_rollouts(root, &mut files)?;
    files.sort();

    let mut ids = BTreeSet::new();
    let mut warnings = Vec::new();
    for path in files {
        match rollout_id(&path) {
            Ok(Some(id)) => {
                ids.insert(id);
            }
            Ok(None) => {}
            Err(error) => warnings.push(format!("skip {}: {error:#}", path.display())),
        }
    }
    Ok((ids, warnings))
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
                .is_some_and(|extension| extension == "jsonl")
        {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn rollout_id(path: &Path) -> Result<Option<String>> {
    let file = File::open(path).with_context(|| format!("open rollout {}", path.display()))?;
    let mut lines = BufReader::new(file).lines();
    let line = lines.next().context("rollout is empty")??;
    let record: SessionRecord =
        serde_json::from_str(&line).context("first rollout record is not valid JSON")?;
    if record.kind != "session_meta" {
        bail!("first rollout record is not session_meta");
    }
    if record.payload.is_subagent() {
        return Ok(None);
    }
    let id = record.payload.id.context("session_meta has no thread ID")?;
    let parsed = Uuid::parse_str(&id).context("session_meta thread ID is not a UUID")?;
    let canonical = parsed.hyphenated().to_string();
    if id != canonical {
        bail!("session_meta thread ID is not canonical");
    }
    Ok(Some(id))
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

impl SessionPayload {
    fn is_subagent(&self) -> bool {
        self.source.as_object().is_some_and(|source| {
            source.contains_key("subagent") || source.contains_key("subAgent")
        })
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
        thread::spawn(move || drain_stderr(stderr_pipe, stderr_tail));

        Ok(Self {
            child,
            stdin: Some(stdin),
            output,
            stderr,
            deadline: Instant::now() + timeout,
            next_id: 0,
        })
    }

    fn call<T: DeserializeOwned>(&mut self, method: &str, params: Value) -> Result<T> {
        self.next_id += 1;
        let id = self.next_id;
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
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
                Ok(ServerOutput::Eof) => bail!(
                    "Codex app-server stopped before {method} replied{}",
                    self.stderr_suffix()
                ),
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
        serde_json::to_writer(&mut *stdin, message)?;
        stdin.write_all(b"\n")?;
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

#[derive(Deserialize)]
struct ThreadReadResult {
    thread: ReadThread,
}

#[derive(Deserialize)]
struct ReadThread {
    #[serde(default)]
    preview: String,
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
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"thread":{{"preview":"shared session"}}}}}}\n' "$id"
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
    fn asks_codex_to_index_a_missing_rollout() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("codex-home");
        fs::create_dir(&home).unwrap();
        write_rollout(&home);
        let codex = temp.path().join("codex");
        fake_codex(&codex, &home);

        let result = reconcile_home(&codex, &home).unwrap();
        assert_eq!(result.already_indexed, 0);
        assert_eq!(result.reconciled, 1);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn skips_a_malformed_rollout_with_a_clear_warning() {
        let temp = tempdir().unwrap();
        let sessions = temp.path().join("sessions");
        fs::create_dir(&sessions).unwrap();
        let path = sessions.join("rollout-broken.jsonl");
        fs::write(&path, "not json\n").unwrap();

        let (ids, warnings) = rollout_ids(&sessions).unwrap();
        assert!(ids.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("first rollout record is not valid JSON"));
    }

    #[test]
    fn does_not_reconcile_subagent_rollouts() {
        let temp = tempdir().unwrap();
        let sessions = temp.path().join("sessions/2026/08/20");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(
            sessions.join(format!("rollout-2026-08-20T10-00-00-{ID}.jsonl")),
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{ID}\",\"source\":{{\"subagent\":{{}}}}}}}}\n"
            ),
        )
        .unwrap();

        let (ids, warnings) = rollout_ids(&temp.path().join("sessions")).unwrap();
        assert!(ids.is_empty());
        assert!(warnings.is_empty());
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

        let first = reconcile_home(Path::new("codex"), &home).unwrap();
        assert_eq!(first.already_indexed + first.reconciled, 1);
        let second = reconcile_home(Path::new("codex"), &home).unwrap();
        assert_eq!(second.already_indexed, 1);
        assert_eq!(second.reconciled, 0);
    }
}
