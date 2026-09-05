use crate::app_server::{state_dir, Connection, Rpc};
use crate::completion::{self, Completion};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use wt_agent_tool_gateway::{CodexTurnStatus, RELAY_SOCKET};

#[derive(Deserialize, Serialize)]
struct TrackedThread {
    thread_id: String,
    settled: BTreeSet<String>,
    pending: BTreeMap<String, Completion>,
}

fn directory() -> Result<PathBuf> {
    let path = state_dir()?.join("threads");
    fs::create_dir_all(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

fn save(path: &Path, tracked: &TrackedThread, create: bool) -> Result<()> {
    let parent = path.parent().context("tracking file has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(temp.as_file_mut(), tracked)?;
    temp.as_file().sync_all()?;
    if create {
        match temp.persist_noclobber(path) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    } else {
        temp.persist(path)?;
    }
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Register before submitting a turn, so a lost submission response cannot orphan its result.
/// Existing terminal history is a baseline; an existing in-flight turn must still be delivered.
pub(crate) fn register(rpc: &mut impl Rpc, thread_id: &str) -> Result<()> {
    let path = directory()?.join(format!("{:x}.json", Sha256::digest(thread_id.as_bytes())));
    if path.try_exists()? {
        return Ok(());
    }
    let snapshot = rpc.call(
        "thread/read",
        json!({ "threadId": thread_id, "includeTurns": true }),
    )?;
    let settled = turns(&snapshot)?
        .iter()
        .filter(|turn| {
            matches!(
                turn.get("status").and_then(Value::as_str),
                Some("completed" | "failed" | "interrupted")
            )
        })
        .map(turn_id)
        .collect::<Result<BTreeSet<_>>>()?;
    save(
        &path,
        &TrackedThread {
            thread_id: thread_id.into(),
            settled,
            pending: BTreeMap::new(),
        },
        true,
    )
}

/// A fresh legacy thread is not materialized/readable until its first user message.
pub(crate) fn register_new(thread_id: &str) -> Result<()> {
    let path = directory()?.join(format!("{:x}.json", Sha256::digest(thread_id.as_bytes())));
    save(
        &path,
        &TrackedThread {
            thread_id: thread_id.into(),
            settled: BTreeSet::new(),
            pending: BTreeMap::new(),
        },
        true,
    )
}

fn turns(snapshot: &Value) -> Result<&Vec<Value>> {
    snapshot
        .pointer("/thread/turns")
        .and_then(Value::as_array)
        .context("thread/read omitted turns")
}

fn turn_id(turn: &Value) -> Result<String> {
    turn.get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("turn has no ID")
}

fn drain_pending(path: &Path, tracked: &mut TrackedThread, relay: &Path) -> Result<()> {
    for id in tracked.pending.keys().cloned().collect::<Vec<_>>() {
        completion::deliver(&tracked.pending[&id], relay)?;
        tracked.pending.remove(&id);
        tracked.settled.insert(id);
        save(path, tracked, false)?;
    }
    Ok(())
}

fn poll_thread(
    rpc: &mut impl Rpc,
    path: &Path,
    relay: &Path,
    subscribed: &mut BTreeSet<String>,
) -> Result<()> {
    let mut tracked: TrackedThread = serde_json::from_reader(File::open(path)?)?;
    drain_pending(path, &mut tracked, relay)?;
    let snapshot = rpc.call(
        "thread/read",
        json!({ "threadId": tracked.thread_id, "includeTurns": true }),
    )?;
    let status = snapshot
        .pointer("/thread/status/type")
        .and_then(Value::as_str);
    if status == Some("active") && !subscribed.contains(&tracked.thread_id) {
        rpc.call("thread/resume", json!({ "threadId": tracked.thread_id }))?;
        subscribed.insert(tracked.thread_id.clone());
    }
    for turn in turns(&snapshot)? {
        let id = turn_id(turn)?;
        if tracked.settled.contains(&id) {
            continue;
        }
        let completion = if turn.get("status").and_then(Value::as_str) == Some("inProgress") {
            if status != Some("notLoaded") {
                continue;
            }
            Completion {
                thread_id: tracked.thread_id.clone(), turn_id: id.clone(), pane_id: None,
                status: CodexTurnStatus::Failed,
                message: "WT recovery: this turn has no loaded Codex runtime; its final outcome is unavailable. Resume the thread before sending new work. WT has not resubmitted the prompt.".into(),
            }
        } else {
            completion::from_turn(&tracked.thread_id, None, turn)?
        };
        // Keep the actual delivery payload even if history changes or Codex goes offline later.
        tracked.pending.insert(id, completion);
        save(path, &tracked, false)?;
        drain_pending(path, &mut tracked, relay)?;
    }
    if matches!(status, Some("idle" | "systemError")) && subscribed.remove(&tracked.thread_id) {
        rpc.call(
            "thread/unsubscribe",
            json!({ "threadId": tracked.thread_id }),
        )?;
    }
    Ok(())
}

pub(crate) fn watch() -> Result<()> {
    let directory = directory()?;
    let lock = File::create(directory.join("worker.lock"))?;
    lock.try_lock()
        .context("completion worker is already running")?;
    let mut connection = None;
    let mut subscribed = BTreeSet::new();
    loop {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let result = (|| {
                // Deliver previously persisted results even while App Server is unavailable.
                let mut tracked = serde_json::from_reader(File::open(&path)?)?;
                drain_pending(&path, &mut tracked, Path::new(RELAY_SOCKET))?;
                if connection.is_none() {
                    connection = Some(Connection::open()?);
                    subscribed.clear();
                }
                poll_thread(
                    connection.as_mut().unwrap(),
                    &path,
                    Path::new(RELAY_SOCKET),
                    &mut subscribed,
                )
            })();
            if let Err(error) = result {
                eprintln!("WT Codex recovery {}: {error:#}", path.display());
                connection = None;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use wt_agent_tool_gateway::{
        read_json_line, write_json_line, ClientRequest, TransportResponse,
    };

    struct Snapshot(Value);
    impl Rpc for Snapshot {
        fn call(&mut self, method: &str, _: Value) -> Result<Value> {
            match method {
                "thread/read" | "thread/resume" => Ok(self.0.clone()),
                "thread/unsubscribe" => Ok(json!({})),
                _ => panic!("recovery must not submit work: {method}"),
            }
        }
    }

    #[test]
    fn in_progress_and_lost_ack_survive_worker_restart_with_durable_payload() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("thread.json");
        let relay = temp.path().join("relay.sock");
        save(
            &path,
            &TrackedThread {
                thread_id: "thread-1".into(),
                settled: BTreeSet::new(),
                pending: BTreeMap::new(),
            },
            true,
        )
        .unwrap();
        let mut running = Snapshot(json!({"thread":{"status":{"type":"active"},
            "turns":[{"id":"turn-1","status":"inProgress"}]}}));
        poll_thread(&mut running, &path, &relay, &mut BTreeSet::new()).unwrap();
        let mut finished = Snapshot(json!({"thread":{"status":{"type":"idle"},
            "turns":[{"id":"turn-1","status":"completed","items":[{"type":"agentMessage","text":"done"}]}]}}));
        // Relay outage leaves the completed payload on disk.
        assert!(poll_thread(&mut finished, &path, &relay, &mut BTreeSet::new()).is_err());
        let saved: TrackedThread = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        assert_eq!(saved.pending["turn-1"].message, "done");

        let listener = UnixListener::bind(&relay).unwrap();
        let server = std::thread::spawn(move || {
            let mut deliveries = Vec::new();
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let request: ClientRequest = read_json_line(&mut stream).unwrap();
                deliveries.push(serde_json::to_value(request).unwrap());
                // First commit's ACK is lost. The next worker must replay the same payload.
                if attempt == 1 {
                    write_json_line(&mut stream, &TransportResponse::ok()).unwrap();
                }
            }
            assert_eq!(deliveries[0], deliveries[1]);
        });
        assert!(poll_thread(&mut finished, &path, &relay, &mut BTreeSet::new()).is_err());
        poll_thread(&mut finished, &path, &relay, &mut BTreeSet::new()).unwrap();
        server.join().unwrap();
        let saved: TrackedThread = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        assert!(saved.pending.is_empty());
        assert!(saved.settled.contains("turn-1"));
        // Repeated polling doesn't redeliver already acknowledged work, even with no relay.
        fs::remove_file(&relay).unwrap();
        poll_thread(&mut finished, &path, &relay, &mut BTreeSet::new()).unwrap();
    }

    #[test]
    fn unloaded_in_progress_history_reports_recovery_failure_without_resubmission() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("thread.json");
        save(
            &path,
            &TrackedThread {
                thread_id: "thread-1".into(),
                settled: BTreeSet::new(),
                pending: BTreeMap::new(),
            },
            true,
        )
        .unwrap();
        let mut snapshot = Snapshot(json!({"thread":{"status":{"type":"notLoaded"},
            "turns":[{"id":"turn-1","status":"inProgress"}]}}));
        assert!(poll_thread(
            &mut snapshot,
            &path,
            &temp.path().join("missing.sock"),
            &mut BTreeSet::new()
        )
        .is_err());
        let saved: TrackedThread = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        let failure = &saved.pending["turn-1"];
        assert_eq!(failure.status, CodexTurnStatus::Failed);
        assert!(failure.message.starts_with("WT recovery:"));
        assert_eq!(failure.turn_id, "turn-1");
    }
}
