use crate::codex::{Connection, Rpc};
use crate::store::{Receipt, Store, Thread, Turn};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub fn execute(directory: &Path, input: &Value) -> Result<Value> {
    ensure!(input["api_version"] == 1, "unsupported API version");
    let request_id = text(input, "request_id")?;
    Uuid::parse_str(request_id).context("request_id must be a UUID")?;
    let operation = text(input, "operation")?;
    let mut store = Store::open(directory)?;
    match operation {
        "read_events" => {
            let after = input["after"]
                .as_u64()
                .context("after must be an unsigned cursor")?;
            let limit = input["limit"].as_u64().unwrap_or(100);
            ensure!(
                (1..=100).contains(&limit),
                "limit must be between 1 and 100"
            );
            let high_water = store.state.events.len() as u64;
            ensure!(after <= high_water, "cursor is beyond the outbox");
            return Ok(
                json!({"events":store.state.events.iter().skip(after as usize)
                .take(limit as usize).collect::<Vec<_>>(), "high_water":high_water}),
            );
        }
        "ack_events" => {
            let through = input["through"]
                .as_u64()
                .context("through must be an unsigned cursor")?;
            ensure!(
                through <= store.state.events.len() as u64,
                "ack exceeds outbox"
            );
            store.state.acknowledged = store.state.acknowledged.max(through);
            store.save()?;
            return Ok(json!({"acknowledged":store.state.acknowledged}));
        }
        "inspect_thread" => {
            let id = text(input, "thread_id")?;
            let thread = store
                .state
                .threads
                .get(id)
                .context("unknown agapi thread")?;
            let mut rpc = Connection::open(directory)?;
            let snapshot = rpc.call(
                "thread/read",
                json!({
                    "threadId":thread.provider_id, "includeTurns":true
                }),
            )?;
            return inspection(id, thread, &snapshot);
        }
        "new_thread" | "send_message" | "resume_thread" | "steer_turn" | "interrupt_turn" => {}
        _ => bail!("unknown agapi operation"),
    }
    if let Some(result) = store.replay(request_id, input)? {
        return Ok(result);
    }
    // Validate and connect before recording the irreversible dispatch boundary.
    let message = if matches!(operation, "new_thread" | "send_message" | "steer_turn") {
        let message = text(input, "message")?;
        ensure!(!message.trim().is_empty(), "message must not be empty");
        Some(message)
    } else {
        None
    };
    let mut rpc = Connection::open(directory)?;
    let thread_id = if operation == "new_thread" {
        Uuid::new_v4().to_string()
    } else {
        let id = text(input, "thread_id")?;
        ensure!(store.state.threads.contains_key(id), "unknown agapi thread");
        id.to_owned()
    };
    if operation == "send_message" {
        let thread = &store.state.threads[&thread_id];
        let snapshot = rpc.call(
            "thread/read",
            json!({"threadId":thread.provider_id,
            "includeTurns":true}),
        )?;
        ensure!(
            snapshot["thread"]["status"]["type"] != "active",
            "thread is busy"
        );
        ensure!(
            thread.turns.values().all(|turn| turn.provider_id.is_some()),
            "thread has an unknown submission; inspect before continuing"
        );
    }
    let provider_turn = if matches!(operation, "steer_turn" | "interrupt_turn") {
        let id = text(input, "turn_id")?;
        Some(
            store.state.threads[&thread_id]
                .turns
                .get(id)
                .and_then(|turn| turn.provider_id.clone())
                .context("unknown agapi turn")?,
        )
    } else {
        None
    };
    store.state.requests.insert(
        request_id.to_owned(),
        Receipt {
            input: input.clone(),
            result: None,
        },
    );
    store.save()?;
    if operation == "new_thread" {
        let workspace = std::fs::read_to_string(directory.join("workspace"))
            .context("agapi serve has not configured a workspace")?;
        let result = rpc.call(
            "thread/start",
            json!({"cwd":workspace,
            "approvalPolicy":"never", "sandbox":"danger-full-access",
            "serviceName":"agapi", "historyMode":"legacy"}),
        )?;
        let provider_id = text(&result["thread"], "id")?.to_owned();
        store.state.threads.insert(
            thread_id.clone(),
            Thread {
                provider_id,
                turns: BTreeMap::new(),
            },
        );
        store.save()?;
    }
    let provider_id = store.state.threads[&thread_id].provider_id.clone();
    let result = match operation {
        "new_thread" | "send_message" => {
            if operation == "send_message" {
                rpc.call("thread/resume", json!({"threadId":provider_id}))?;
            }
            let turn_id = Uuid::new_v4().to_string();
            store
                .state
                .threads
                .get_mut(&thread_id)
                .unwrap()
                .turns
                .insert(
                    turn_id.clone(),
                    Turn {
                        provider_id: None,
                        terminal: false,
                    },
                );
            store.save()?;
            let result = rpc.call(
                "turn/start",
                json!({"threadId":provider_id,
                "input":[{"type":"text","text":message.unwrap()}]}),
            )?;
            store
                .state
                .threads
                .get_mut(&thread_id)
                .unwrap()
                .turns
                .get_mut(&turn_id)
                .unwrap()
                .provider_id = Some(text(&result["turn"], "id")?.to_owned());
            json!({"thread_id":thread_id,"turn_id":turn_id,"delivery":"started"})
        }
        "resume_thread" => {
            let snapshot = rpc.call("thread/resume", json!({"threadId":provider_id}))?;
            inspection(&thread_id, &store.state.threads[&thread_id], &snapshot)?
        }
        "steer_turn" => {
            rpc.call(
                "turn/steer",
                json!({"threadId":provider_id,
                "expectedTurnId":provider_turn,"input":[{"type":"text","text":message.unwrap()}]}),
            )?;
            json!({"thread_id":thread_id,"turn_id":input["turn_id"],"delivery":"steered"})
        }
        "interrupt_turn" => {
            rpc.call(
                "turn/interrupt",
                json!({"threadId":provider_id,"turnId":provider_turn}),
            )?;
            json!({"thread_id":thread_id,"turn_id":input["turn_id"],"delivery":"interrupt_requested"})
        }
        _ => unreachable!(),
    };
    store.state.requests.get_mut(request_id).unwrap().result = Some(result.clone());
    store.save()?;
    Ok(result)
}

fn inspection(id: &str, thread: &Thread, snapshot: &Value) -> Result<Value> {
    let status = snapshot["thread"]["status"]["type"]
        .as_str()
        .context("missing thread status")?;
    let active = snapshot["thread"]["turns"]
        .as_array()
        .and_then(|turns| {
            turns
                .iter()
                .rev()
                .find(|turn| turn["status"] == "inProgress")
        })
        .and_then(|turn| {
            thread
                .turns
                .iter()
                .find(|(_, stored)| stored.provider_id.as_deref() == turn["id"].as_str())
                .map(|(id, _)| id)
        });
    let status = match status {
        "active" => "active",
        "idle" | "notLoaded" => "idle",
        "systemError" => "error",
        _ => bail!("unsupported provider thread status: {status}"),
    };
    Ok(json!({"thread_id":id,"status":status,"active_turn_id":active}))
}

pub fn reconcile(directory: &Path) -> Result<()> {
    let mut store = Store::open(directory)?;
    if store.state.threads.is_empty() {
        return Ok(());
    }
    let mut rpc = Connection::open(directory)?;
    let mut events = Vec::new();
    for (thread_id, thread) in &mut store.state.threads {
        if thread.turns.values().all(|turn| turn.terminal) {
            continue;
        }
        let snapshot = rpc.call(
            "thread/read",
            json!({"threadId":thread.provider_id,
            "includeTurns":true}),
        )?;
        let turns = snapshot["thread"]["turns"]
            .as_array()
            .context("missing provider turns")?;
        for (turn_id, stored) in &mut thread.turns {
            if stored.terminal {
                continue;
            }
            let Some(turn) = turns.iter().find(|turn| {
                stored.provider_id.is_some() && stored.provider_id.as_deref() == turn["id"].as_str()
            }) else {
                continue;
            };
            let (kind, fallback) = match turn["status"].as_str() {
                Some("completed") => ("completed", "Turn completed"),
                Some("failed") => ("failed", "Turn failed"),
                Some("interrupted") => ("failed", "Turn interrupted"),
                Some("inProgress") if snapshot["thread"]["status"]["type"] == "notLoaded" => (
                    "failed",
                    "Provider stopped during turn; prompt was not replayed",
                ),
                _ => continue,
            };
            let message = turn["items"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .rev()
                        .find(|item| item["type"] == "agentMessage")
                })
                .and_then(|item| item["text"].as_str())
                .or_else(|| turn["error"]["message"].as_str())
                .unwrap_or(fallback);
            events.push(json!({"thread_id":thread_id,"turn_id":turn_id,"kind":kind,
                "text":message,"created_at_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH)?
                    .as_millis() as u64}));
            stored.terminal = true;
        }
    }
    for mut event in events {
        event["event_id"] = json!(store.state.events.len() + 1);
        store.state.events.push(event);
    }
    store.save()
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("{key} must be a string"))
}
