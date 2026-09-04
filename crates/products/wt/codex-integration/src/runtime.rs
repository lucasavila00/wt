use crate::app_server::{Connection, Rpc};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const TMUX: &str = "/usr/bin/tmux";
const BYOBU: &str = "/usr/bin/byobu-tmux";
const TMUX_CONFIG: &str = "/usr/local/share/wt-tmux.conf";
const TMUX_SESSION: &str = "wt-host";
const THREAD_OPTION: &str = "@wt_codex_thread_id";
const WINDOW_NAME: &str = "codex";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct StartOutput {
    pub thread_id: String,
    pub turn_id: String,
    pub pane_id: Option<String>,
    pub window_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct InspectOutput {
    pub status: RuntimeStatus,
    pub active_turn_id: Option<String>,
    pub pane_id: Option<String>,
    pub window_name: Option<String>,
    pub screen: Option<String>,
    pub observed_at_unix_ms: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeStatus {
    Active,
    Idle,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct SendOutput {
    pub turn_id: String,
    pub delivery: MessageDelivery,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MessageDelivery {
    Steered,
    Started,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThreadState {
    status: RuntimeStatus,
    active_turn_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Pane {
    pane_id: String,
    window_name: String,
}

pub(crate) fn start(message: &str) -> Result<StartOutput> {
    let mut rpc = Connection::open()?;
    let thread_id = start_thread(&mut rpc)?;
    crate::tracking::register_new(&thread_id)?;
    let turn_id = start_turn(&mut rpc, &thread_id, message)?;
    let pane = optional_pane(create_pane(&thread_id));
    Ok(StartOutput {
        thread_id,
        turn_id,
        pane_id: pane.as_ref().map(|pane| pane.pane_id.clone()),
        window_name: pane.map(|pane| pane.window_name),
    })
}

pub(crate) fn inspect(thread_id: &str) -> Result<InspectOutput> {
    let mut rpc = Connection::open()?;
    let state = inspect_thread(&mut rpc, thread_id)?;
    let pane = optional_pane(find_pane(thread_id));
    let screen = pane
        .as_ref()
        .and_then(|pane| capture_screen(&pane.pane_id).ok());
    Ok(InspectOutput {
        status: state.status,
        active_turn_id: state.active_turn_id,
        pane_id: pane.as_ref().map(|pane| pane.pane_id.clone()),
        window_name: pane.map(|pane| pane.window_name),
        screen,
        observed_at_unix_ms: now_unix_ms()?,
    })
}

pub(crate) fn resume(thread_id: &str) -> Result<InspectOutput> {
    let mut rpc = Connection::open()?;
    crate::tracking::register(&mut rpc, thread_id)?;
    rpc.call("thread/resume", json!({ "threadId": thread_id }))?;
    optional_pane(find_pane(thread_id).or_else(|_| create_pane(thread_id)));
    inspect(thread_id)
}

pub(crate) fn send(thread_id: &str, message: &str) -> Result<SendOutput> {
    let mut rpc = Connection::open()?;
    crate::tracking::register(&mut rpc, thread_id)?;
    rpc.call("thread/resume", json!({ "threadId": thread_id }))?;
    let output = send_message(&mut rpc, thread_id, message)?;
    Ok(output)
}

fn optional_pane(result: Result<Pane>) -> Option<Pane> {
    result
        .map_err(|error| eprintln!("WT Codex presentation unavailable: {error:#}"))
        .ok()
}

fn start_thread(rpc: &mut impl Rpc) -> Result<String> {
    let started = rpc.call(
        "thread/start",
        json!({
            "cwd": "/home/wt",
            "approvalPolicy": "never",
            "sandbox": "danger-full-access",
            "serviceName": "wt",
            "historyMode": "legacy"
        }),
    )?;
    required_string(&started, "/thread/id", "thread/start thread ID")
}

fn start_turn(rpc: &mut impl Rpc, thread_id: &str, message: &str) -> Result<String> {
    let turn = rpc.call("turn/start", text_turn_params(thread_id, message))?;
    required_string(&turn, "/turn/id", "turn/start turn ID")
}

fn inspect_thread(rpc: &mut impl Rpc, thread_id: &str) -> Result<ThreadState> {
    let result = rpc
        .call(
            "thread/read",
            json!({ "threadId": thread_id, "includeTurns": true }),
        )
        .with_context(|| format!("Codex thread not found: {thread_id}"))?;
    parse_thread_state(result.get("thread").context("thread/read has no thread")?)
}

fn send_message(rpc: &mut impl Rpc, thread_id: &str, message: &str) -> Result<SendOutput> {
    let state = inspect_thread(rpc, thread_id)?;
    match (state.status, state.active_turn_id) {
        (RuntimeStatus::Active, Some(turn_id)) => {
            let result = rpc.call(
                "turn/steer",
                json!({
                    "threadId": thread_id,
                    "expectedTurnId": turn_id,
                    "input": [{ "type": "text", "text": message }]
                }),
            )?;
            Ok(SendOutput {
                turn_id: required_string(&result, "/turnId", "turn/steer turn ID")?,
                delivery: MessageDelivery::Steered,
            })
        }
        (RuntimeStatus::Idle | RuntimeStatus::Error, _) => {
            rpc.call("thread/resume", json!({ "threadId": thread_id }))?;
            let result = rpc.call("turn/start", text_turn_params(thread_id, message))?;
            Ok(SendOutput {
                turn_id: required_string(&result, "/turn/id", "turn/start turn ID")?,
                delivery: MessageDelivery::Started,
            })
        }
        (RuntimeStatus::Active, None) => bail!("active Codex thread has no active turn"),
    }
}

fn text_turn_params(thread_id: &str, message: &str) -> Value {
    json!({
        "threadId": thread_id,
        "input": [{ "type": "text", "text": message }]
    })
}

fn parse_thread_state(thread: &Value) -> Result<ThreadState> {
    let status = match thread.pointer("/status/type").and_then(Value::as_str) {
        Some("active") => RuntimeStatus::Active,
        Some("idle" | "notLoaded") => RuntimeStatus::Idle,
        Some("systemError") => RuntimeStatus::Error,
        other => bail!("unknown Codex thread status: {other:?}"),
    };
    let active_turn_id = if status == RuntimeStatus::Active {
        thread
            .get("turns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .rev()
            .find(|turn| turn.get("status").and_then(Value::as_str) == Some("inProgress"))
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    } else {
        None
    };
    Ok(ThreadState {
        status,
        active_turn_id,
    })
}

fn create_pane(thread_id: &str) -> Result<Pane> {
    let codex = crate::real_codex()?;
    ensure_tmux_session()?;
    let output = Command::new(TMUX)
        .args([
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{pane_id}\t#{window_name}",
            "-t",
            TMUX_SESSION,
            "-n",
            WINDOW_NAME,
        ])
        .arg(codex)
        .args([
            "--remote",
            &crate::app_server::endpoint()?,
            "resume",
            thread_id,
        ])
        .output()
        .context("create Codex Byobu window")?;
    if !output.status.success() {
        bail!(
            "create Codex Byobu window: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    let pane = parse_created_pane(&output.stdout)?;
    let status = Command::new(TMUX)
        .args([
            "set-option",
            "-p",
            "-t",
            &pane.pane_id,
            THREAD_OPTION,
            thread_id,
        ])
        .status()
        .context("tag Codex Byobu pane")?;
    if !status.success() {
        bail!("tag Codex Byobu pane")
    }
    Ok(pane)
}

fn ensure_tmux_session() -> Result<()> {
    if Command::new(TMUX)
        .args(["has-session", "-t", TMUX_SESSION])
        .status()
        .context("inspect WT Byobu session")?
        .success()
    {
        return Ok(());
    }
    let output = Command::new(BYOBU)
        .args([
            "-f",
            TMUX_CONFIG,
            "new-session",
            "-d",
            "-s",
            TMUX_SESSION,
            "/bin/bash",
        ])
        .output()
        .context("create WT Byobu session")?;
    if !output.status.success()
        && !Command::new(TMUX)
            .args(["has-session", "-t", TMUX_SESSION])
            .status()
            .is_ok_and(|status| status.success())
    {
        bail!(
            "create WT Byobu session: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    Ok(())
}

fn find_pane(thread_id: &str) -> Result<Pane> {
    let output = Command::new(TMUX)
        .args([
            "list-panes",
            "-s",
            "-t",
            TMUX_SESSION,
            "-F",
            "#{pane_id}\t#{window_name}\t#{@wt_codex_thread_id}\t#{pane_dead}",
        ])
        .output()
        .context("list Codex Byobu panes")?;
    if !output.status.success() {
        bail!("Codex pane not found for thread {thread_id}")
    }
    for line in output.stdout.split(|byte| *byte == b'\n') {
        let Some((pane, stored_thread, dead)) = parse_pane(line) else {
            continue;
        };
        if stored_thread == thread_id && dead == "0" {
            return Ok(pane);
        }
    }
    bail!("Codex pane not found for thread {thread_id}")
}

fn parse_created_pane(output: &[u8]) -> Result<Pane> {
    let text = std::str::from_utf8(output)
        .context("decode created Codex pane")?
        .trim_end();
    let (pane_id, window_name) = text
        .split_once('\t')
        .context("created Codex pane has invalid metadata")?;
    Ok(Pane {
        pane_id: pane_id.to_owned(),
        window_name: window_name.to_owned(),
    })
}

fn parse_pane(line: &[u8]) -> Option<(Pane, &str, &str)> {
    let line = std::str::from_utf8(line).ok()?;
    let (pane_id, rest) = line.split_once('\t')?;
    let (window_name, rest) = rest.split_once('\t')?;
    let (thread_id, dead) = rest.split_once('\t')?;
    Some((
        Pane {
            pane_id: pane_id.to_owned(),
            window_name: window_name.to_owned(),
        },
        thread_id,
        dead,
    ))
}

fn capture_screen(pane_id: &str) -> Result<String> {
    let output = Command::new(TMUX)
        .args(["capture-pane", "-p", "-e", "-t", pane_id])
        .output()
        .context("capture Codex Byobu screen")?;
    if !output.status.success() {
        bail!("capture Codex Byobu screen")
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn now_unix_ms() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_millis(),
    )
    .context("system time is too large")
}

fn required_string(value: &Value, pointer: &str, description: &str) -> Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("Codex app-server response has no {description}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::from_turn as completion_from_turn;
    use std::collections::VecDeque;
    use wt_agent_tool_gateway::CodexTurnStatus;

    struct FakeRpc {
        replies: VecDeque<(&'static str, Value)>,
    }

    impl Rpc for FakeRpc {
        fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
            let (expected, result) = self.replies.pop_front().unwrap();
            assert_eq!(method, expected);
            Ok(result)
        }
    }

    #[test]
    fn active_send_steers_the_current_turn() {
        let mut rpc = FakeRpc {
            replies: VecDeque::from([
                (
                    "thread/read",
                    json!({ "thread": { "status": { "type": "active", "activeFlags": [] }, "turns": [{ "id": "turn-1", "status": "inProgress" }] } }),
                ),
                ("turn/steer", json!({ "turnId": "turn-1" })),
            ]),
        };
        assert_eq!(
            send_message(&mut rpc, "thread-1", "more").unwrap(),
            SendOutput {
                turn_id: "turn-1".into(),
                delivery: MessageDelivery::Steered,
            }
        );
    }

    #[test]
    fn idle_send_starts_the_next_turn() {
        let mut rpc = FakeRpc {
            replies: VecDeque::from([
                (
                    "thread/read",
                    json!({ "thread": { "status": { "type": "idle" }, "turns": [] } }),
                ),
                ("thread/resume", json!({ "thread": { "id": "thread-1" } })),
                ("turn/start", json!({ "turn": { "id": "turn-2" } })),
            ]),
        };
        assert_eq!(
            send_message(&mut rpc, "thread-1", "next").unwrap(),
            SendOutput {
                turn_id: "turn-2".into(),
                delivery: MessageDelivery::Started,
            }
        );
    }

    #[test]
    fn completed_turn_uses_the_final_agent_message() {
        let completion = completion_from_turn(
            "thread-1",
            Some("%3"),
            &json!({
                "id": "turn-1",
                "status": "completed",
                "items": [
                    { "type": "agentMessage", "text": "draft" },
                    { "type": "agentMessage", "text": "done" }
                ]
            }),
        )
        .unwrap();
        assert_eq!(completion.thread_id, "thread-1");
        assert_eq!(completion.turn_id, "turn-1");
        assert_eq!(completion.pane_id.as_deref(), Some("%3"));
        assert_eq!(completion.status, CodexTurnStatus::Completed);
        assert_eq!(completion.message, "done");
    }

    #[test]
    fn completed_turn_always_has_mailbox_text() {
        let completion = completion_from_turn(
            "thread-1",
            Some("%3"),
            &json!({ "id": "turn-1", "status": "completed", "items": [] }),
        )
        .unwrap();

        assert_eq!(completion.message, "Codex turn completed");
    }

    #[test]
    fn pane_metadata_carries_the_runtime_thread_mapping() {
        assert_eq!(
            parse_pane(b"%7\tcodex\tthread-1\t0").unwrap(),
            (
                Pane {
                    pane_id: "%7".into(),
                    window_name: "codex".into()
                },
                "thread-1",
                "0"
            )
        );
    }
}
