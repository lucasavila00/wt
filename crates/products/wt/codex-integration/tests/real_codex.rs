//! No model requests or credentials: exercise the installed Codex protocol, not a test peer.
use serde_json::{json, Value};
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
            assert!(response.get("error").is_none(), "{response}");
            return response["result"].clone();
        }
    }
}

#[test]
#[ignore = "CI installs the real Codex executable; no model calls or authentication required"]
fn documented_unix_transport_reads_and_resumes_a_real_thread() {
    let binary = std::env::var_os("WT_CODEX_TEST_BINARY").expect("installed Codex binary path");
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".codex")).unwrap();
    let state = root.path().join(".local/state/wt/codex");
    std::fs::create_dir_all(&state).unwrap();
    let path = state.join("app-server.sock");
    let mut server = Server(
        Command::new(binary)
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
        json!({"clientInfo":{"name":"wt_ci","version":"1"}}),
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
            "cwd":root.path(), "approvalPolicy":"never", "sandbox":"danger-full-access"
        }),
    );
    let thread = started["thread"]["id"].as_str().unwrap();
    let snapshot = call(
        &mut socket,
        3,
        "thread/read",
        json!({"threadId":thread,"includeTurns":true}),
    );
    assert_eq!(snapshot["thread"]["turns"], json!([]));
    let resumed = call(&mut socket, 4, "thread/resume", json!({"threadId":thread}));
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
    assert_eq!(inspected["status"], "idle");
    assert!(inspected["pane_id"].is_null());
}
