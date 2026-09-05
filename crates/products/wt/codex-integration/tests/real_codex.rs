//! Real Codex with a rejecting localhost provider: no external model service or credentials.
#[path = "support/reject_provider.rs"]
mod reject_provider;
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn call(socket: &mut WebSocket<UnixStream>, id: u64, method: &str, params: Value) -> Value {
    socket
        .send(Message::Text(
            json!({"id":id,"method":method,"params":params})
                .to_string()
                .into(),
        ))
        .unwrap();
    loop {
        let Message::Text(text) = socket.read().unwrap() else {
            continue;
        };
        let response: Value = serde_json::from_str(&text).unwrap();
        if response.get("method").is_none() && response["id"] == id {
            eprintln!("{method}: {response}");
            assert!(response.get("error").is_none(), "{response}");
            return response["result"].clone();
        }
    }
}

#[test]
#[ignore = "CI installs real Codex; model requests go only to a rejecting localhost fixture"]
fn documented_unix_transport_reads_and_resumes_a_real_thread() {
    let binary = std::env::var_os("WT_CODEX_TEST_BINARY").expect("installed Codex binary path");
    let version = Command::new(&binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!(
            "codex-cli {}",
            include_str!("../../../../../.codex-version").trim()
        )
    );
    let root = tempfile::tempdir().unwrap();
    let provider = reject_provider::RejectProvider::new();
    std::fs::create_dir(root.path().join(".codex")).unwrap();
    let state = root.path().join(".local/state/wt/codex");
    std::fs::create_dir_all(&state).unwrap();
    let path = state.join("app-server.sock");
    let mut server = Server(
        Command::new(binary)
            .args([
                "-c",
                "model='wt-compatibility-fixture'",
                "-c",
                "model_provider='wt_ci'",
                "-c",
                &provider.config(),
            ])
            .args([
                "app-server",
                "--listen",
                &format!("unix://{}", path.display()),
            ])
            .env("HOME", root.path())
            .env("CODEX_HOME", root.path().join(".codex"))
            .current_dir(root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let stream = loop {
        if let Ok(stream) = UnixStream::connect(&path) {
            break stream;
        }
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "Codex App Server exited"
        );
        assert!(Instant::now() < deadline, "Codex socket not ready");
        std::thread::sleep(Duration::from_millis(50));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let (mut socket, _) = tungstenite::client("ws://localhost/", stream).unwrap();
    call(
        &mut socket,
        1,
        "initialize",
        json!({"capabilities":{"experimentalApi":true},"clientInfo":{"name":"wt_ci","version":"1"}}),
    );
    socket
        .send(Message::Text(
            json!({"method":"initialized","params":{}})
                .to_string()
                .into(),
        ))
        .unwrap();
    let started = call(
        &mut socket,
        2,
        "thread/start",
        json!({
            "cwd":root.path(), "approvalPolicy":"never", "sandbox":"danger-full-access",
            "historyMode":"legacy", "serviceName":"wt"
        }),
    );
    let thread = started["thread"]["id"].as_str().unwrap();
    assert_eq!(started["thread"]["historyMode"], "legacy");
    // Materialize real history; the only model endpoint deliberately rejects work.
    let turn = call(
        &mut socket,
        3,
        "turn/start",
        json!({"threadId":thread,
        "input":[{"type":"text","text":"WT compatibility fixture"}]}),
    );
    let turn_id = turn["turn"]["id"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while provider.requests() == 0 {
        assert!(
            Instant::now() < deadline,
            "Codex did not contact the local fixture"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut request_id = 4;
    loop {
        let snapshot = call(
            &mut socket,
            request_id,
            "thread/read",
            json!({"threadId":thread,"includeTurns":true}),
        );
        request_id += 1;
        if snapshot["thread"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|turn| turn["id"] == turn_id && turn["status"] == "failed")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Codex did not persist the failed turn"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(provider.requests(), 1);
    let resumed = call(
        &mut socket,
        request_id,
        "thread/resume",
        json!({"threadId":thread}),
    );
    assert_eq!(resumed["thread"]["id"], thread);

    // Exercise WT's real client independently of both this connection and a terminal pane.
    let inspected = Command::new(env!("CARGO_BIN_EXE_wt-codex-integration"))
        .args(["runtime-inspect", thread])
        .env("HOME", root.path())
        .env("CODEX_HOME", root.path().join(".codex"))
        .env("TMUX_TMPDIR", root.path())
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspected["status"], "error");
    assert!(inspected["pane_id"].is_null());

    // A failed turn is not an unusable thread. An explicit new message may continue it.
    let mut sent = Command::new(env!("CARGO_BIN_EXE_wt-codex-integration"))
        .args(["runtime-send", thread])
        .env("HOME", root.path())
        .env("CODEX_HOME", root.path().join(".codex"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    sent.stdin
        .take()
        .unwrap()
        .write_all(b"explicit continuation")
        .unwrap();
    let sent = sent.wait_with_output().unwrap();
    assert!(
        sent.status.success(),
        "{}",
        String::from_utf8_lossy(&sent.stderr)
    );
    let sent: Value = serde_json::from_slice(&sent.stdout).unwrap();
    assert_eq!(sent["delivery"], "started");
    assert_ne!(sent["turn_id"], turn_id);
    let deadline = Instant::now() + Duration::from_secs(30);
    while provider.requests() < 2 {
        assert!(
            Instant::now() < deadline,
            "explicit continuation did not reach local fixture"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Finish the rejected continuation before holding the next real model request open.
    loop {
        let snapshot = call(
            &mut socket,
            request_id,
            "thread/read",
            json!({"threadId":thread,"includeTurns":true}),
        );
        request_id += 1;
        if snapshot["thread"]["status"]["type"] != "active" {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    provider.hold(true);
    let started = call(
        &mut socket,
        request_id,
        "turn/start",
        json!({"threadId":thread,"input":[{"type":"text","text":"wait for explicit control"}]}),
    );
    request_id += 1;
    let active_turn = started["turn"]["id"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while provider.requests() < 3 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    for (operation, target, message, expected_success) in [
        ("runtime-steer", "stale-turn", Some("must reject"), false),
        ("runtime-steer", active_turn, Some("focus on tests"), true),
        ("runtime-interrupt", active_turn, None, true),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wt-codex-integration"))
            .args([operation, thread, target])
            .env("HOME", root.path())
            .env("CODEX_HOME", root.path().join(".codex"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(message) = message {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(message.as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.success(),
            expected_success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    loop {
        let snapshot = call(
            &mut socket,
            request_id,
            "thread/read",
            json!({"threadId":thread,"includeTurns":true}),
        );
        request_id += 1;
        if snapshot["thread"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|turn| turn["id"] == active_turn && turn["status"] == "interrupted")
        {
            break;
        }
        assert!(Instant::now() < deadline, "interruption was not persisted");
        std::thread::sleep(Duration::from_millis(20));
    }
    provider.hold(false);
}
