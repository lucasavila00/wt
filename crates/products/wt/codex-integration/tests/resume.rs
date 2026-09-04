use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::process::{Command, Output, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tungstenite::Message;

struct Runtime {
    root: tempfile::TempDir,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl Runtime {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join(".local/state/wt/codex");
        fs::create_dir_all(&state).unwrap();
        let listener = UnixListener::bind(state.join("app-server.sock")).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        // Real Unix WebSocket transport; a deterministic protocol peer supplies Codex state.
        let server = std::thread::spawn(move || {
            let mut connections = Vec::new();
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let log = log.clone();
                connections.push(std::thread::spawn(move || {
                    let Ok(mut socket) = tungstenite::accept(stream.unwrap()) else {
                        return;
                    };
                    while let Ok(Message::Text(text)) = socket.read() {
                        let request: Value = serde_json::from_str(&text).unwrap();
                        log.lock().unwrap().push(request.clone());
                        let Some(id) = request.get("id") else {
                            continue;
                        };
                        let response =
                            if request.pointer("/params/threadId").and_then(Value::as_str)
                                == Some("missing")
                            {
                                json!({"id": id, "error": {"message": "thread not found"}})
                            } else {
                                let result = match request["method"].as_str().unwrap() {
                                    "initialize" => json!({}),
                                    "thread/start" => {
                                        json!({"thread":{"id":"thread-1","historyMode":"legacy"}})
                                    }
                                    "thread/read" | "thread/resume" => json!({"thread": {
                                        "id": "thread-1", "status": {"type": "idle"}, "turns": []
                                    }}),
                                    "turn/start" => json!({"turn": {"id": "turn-2"}}),
                                    other => panic!("unexpected RPC {other}"),
                                };
                                json!({"id": id, "result": result})
                            };
                        // Events interleaved with RPC replies must not corrupt response matching.
                        if socket
                            .send(Message::Text(
                                json!({"method":"thread/status/changed","params":{}})
                                    .to_string()
                                    .into(),
                            ))
                            .is_err()
                        {
                            break;
                        }
                        if socket
                            .send(Message::Text(response.to_string().into()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
            }
            for connection in connections {
                connection.join().unwrap();
            }
        });
        Self {
            root,
            requests,
            stop,
            server: Some(server),
        }
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .env("HOME", self.root.path())
            .env("CODEX_HOME", self.root.path().join(".codex"))
            .env("TMUX_TMPDIR", self.root.path())
            .env_remove("TMUX");
        command
    }

    fn run(&self, operation: &str, thread: &str, message: Option<&str>) -> Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_wt-codex-integration"));
        command.arg(operation);
        if operation != "runtime-start" {
            command.arg(thread);
        }
        let mut child = command
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
        child.wait_with_output().unwrap()
    }

    fn state(&self, operation: &str) -> Value {
        let output = self.run(operation, "thread-1", None);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

#[test]
fn new_thread_is_tracked_before_submission_without_reading_unmaterialized_history() {
    let runtime = Runtime::new();
    // No Codex TUI executable and no tmux server: semantic startup must still succeed.
    let output = runtime.run("runtime-start", "", Some("work"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let started: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(started["thread_id"], "thread-1");
    assert_eq!(started["turn_id"], "turn-2");
    assert!(started["pane_id"].is_null());
    let requests = runtime.requests.lock().unwrap();
    let start = requests
        .iter()
        .find(|request| request["method"] == "thread/start")
        .unwrap();
    assert_eq!(start["params"]["historyMode"], "legacy");
    assert!(!requests
        .iter()
        .any(|request| request["method"] == "thread/read" || request["method"] == "thread/resume"));
    let files = fs::read_dir(runtime.root.path().join(".local/state/wt/codex/threads")).unwrap();
    let tracking = files
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .unwrap();
    let tracked: Value = serde_json::from_slice(&fs::read(tracking).unwrap()).unwrap();
    assert_eq!(tracked["thread_id"], "thread-1");
    assert_eq!(tracked["settled"], json!([]));
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.command("/usr/bin/tmux").arg("kill-server").output();
        self.stop.store(true, Ordering::SeqCst);
        let _ = UnixStream::connect(
            self.root
                .path()
                .join(".local/state/wt/codex/app-server.sock"),
        );
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

#[test]
fn thread_operations_survive_pane_loss_and_resume_only_restores_presentation() {
    let runtime = Runtime::new();
    let codex = runtime
        .root
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(codex.parent().unwrap()).unwrap();
    fs::write(&codex, "#!/bin/sh\ncase \"$*\" in\n  '--remote unix://'*' resume thread-1') exec sleep 120 ;;\n  *) exit 64 ;;\nesac\n").unwrap();
    fs::set_permissions(&codex, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(runtime
        .command("/usr/bin/tmux")
        .args([
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "wt-host",
            "sleep 120"
        ])
        .status()
        .unwrap()
        .success());

    let first = runtime.state("runtime-resume")["pane_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(runtime.state("runtime-resume")["pane_id"], first);
    assert!(!runtime
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|request| request["method"] == "turn/start"));
    assert!(runtime
        .command("/usr/bin/tmux")
        .args(["kill-pane", "-t", &first])
        .status()
        .unwrap()
        .success());

    let inspection = runtime.state("runtime-inspect");
    assert_eq!(inspection["status"], "idle");
    assert!(inspection["pane_id"].is_null());
    assert!(inspection["screen"].is_null());
    let sent = runtime.run("runtime-send", "thread-1", Some("continue"));
    assert!(
        sent.status.success(),
        "{}",
        String::from_utf8_lossy(&sent.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&sent.stdout).unwrap()["turn_id"],
        "turn-2"
    );

    let recovered = runtime.state("runtime-resume")["pane_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(recovered, first);
    assert_eq!(runtime.state("runtime-resume")["pane_id"], recovered);
    let missing = runtime.run("runtime-resume", "missing", None);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("thread not found"));
    let panes = runtime
        .command("/usr/bin/tmux")
        .args(["list-panes", "-a", "-F", "#{pane_id}"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(panes.stdout).unwrap().lines().count(), 2);
    assert_eq!(
        runtime
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request["method"] == "turn/start")
            .count(),
        1
    );
}
