#[path = "support/reject_provider.rs"]
mod reject_provider;

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status();
        let _ = self.0.wait();
    }
}

fn request(root: &Path, mut input: Value) -> Value {
    input["api_version"] = json!(1);
    if input["request_id"].is_null() {
        input["request_id"] = json!(uuid::Uuid::new_v4());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_agapi"))
        .arg("--state-dir")
        .arg(root.join("state"))
        .arg("api")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["request_id"], input["request_id"]);
    assert_eq!(response["api_version"], 1);
    assert_eq!(
        output.status.success(),
        response["outcome"] == "ok",
        "{response}"
    );
    response
}

#[test]
fn protocol_and_version_errors_cross_the_executable_boundary() {
    let root = tempfile::tempdir().unwrap();
    let result = request(root.path(), json!({"operation":"unknown"}));
    assert_eq!(result["outcome"], "error");
    let output = Command::new(env!("CARGO_BIN_EXE_agapi"))
        .arg("--state-dir")
        .arg(root.path().join("state"))
        .args(["serve", "--codex", "/usr/bin/true", "--workspace"])
        .arg(root.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8(output.stderr).unwrap(), @
        "unsupported Codex version; agapi requires 0.153.3");
    let events = request(root.path(), json!({"operation":"read_events","after":0}));
    assert_eq!(events["result"], json!({"events":[],"high_water":0}));
}

#[test]
#[ignore = "requires AGAPI_CODEX_TEST_BINARY; all model traffic uses localhost"]
fn real_codex_submission_replay_outbox_ack_and_restart() {
    let binary = std::env::var_os("AGAPI_CODEX_TEST_BINARY").expect("installed Codex binary");
    let root = tempfile::tempdir().unwrap();
    let provider = reject_provider::RejectProvider::new();
    provider.hold(false);
    let codex_home = root.path().join("codex");
    std::fs::create_dir(&codex_home).unwrap();
    std::fs::write(
        codex_home.join("config.toml"),
        format!(
            "model='fixture'\nmodel_provider='wt_ci'\n{}\n",
            provider.config()
        ),
    )
    .unwrap();
    let start = || {
        Server(
            Command::new(env!("CARGO_BIN_EXE_agapi"))
                .arg("--state-dir")
                .arg(root.path().join("state"))
                .arg("serve")
                .arg("--codex")
                .arg(&binary)
                .arg("--workspace")
                .arg(root.path())
                .env("HOME", root.path())
                .env("CODEX_HOME", &codex_home)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };
    let mut server = start();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !root.path().join("state/codex.sock").exists() {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "agapi exited before readiness"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let input = json!({"operation":"new_thread","request_id":uuid::Uuid::new_v4(),
        "message":"report a deterministic provider failure"});
    let result = request(root.path(), input.clone());
    assert_eq!(result["outcome"], "ok", "{result}");
    uuid::Uuid::parse_str(result["result"]["thread_id"].as_str().unwrap()).unwrap();
    assert_eq!(request(root.path(), input.clone()), result);
    let mut changed = input;
    changed["message"] = json!("different task");
    assert_eq!(request(root.path(), changed)["outcome"], "error");
    let events = loop {
        let events = request(root.path(), json!({"operation":"read_events","after":0}));
        if events["result"]["high_water"] == 1 {
            break events["result"].clone();
        }
        assert!(Instant::now() < deadline, "{events}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(provider.requests(), 1);
    assert_eq!(events["events"][0]["kind"], "failed");
    assert_eq!(
        events["events"][0]["thread_id"],
        result["result"]["thread_id"]
    );
    assert_eq!(events["events"][0]["turn_id"], result["result"]["turn_id"]);
    let ack = request(root.path(), json!({"operation":"ack_events","through":1}));
    assert_eq!(ack["result"]["acknowledged"], 1);
    let thread = &result["result"]["thread_id"];
    let resumed = request(
        root.path(),
        json!({"operation":"resume_thread","thread_id":thread}),
    );
    assert_eq!(resumed["outcome"], "ok", "{resumed}");
    provider.hold(true);
    let sent = request(
        root.path(),
        json!({"operation":"send_message","thread_id":thread,
        "message":"wait for explicit live control"}),
    );
    assert_eq!(sent["outcome"], "ok", "{sent}");
    let turn = &sent["result"]["turn_id"];
    assert_ne!(*turn, result["result"]["turn_id"]);
    let stale = request(
        root.path(),
        json!({"operation":"steer_turn","thread_id":thread,
        "turn_id":"unknown","message":"reject stale target"}),
    );
    assert_eq!(stale["outcome"], "error");
    let steered = request(
        root.path(),
        json!({"operation":"steer_turn","thread_id":thread,
        "turn_id":turn,"message":"focus on the ADR"}),
    );
    assert_eq!(steered["outcome"], "ok", "{steered}");
    let interrupted = request(
        root.path(),
        json!({"operation":"interrupt_turn",
        "thread_id":thread,"turn_id":turn}),
    );
    assert_eq!(interrupted["outcome"], "ok", "{interrupted}");
    provider.hold(false);
    let events = loop {
        let page = request(root.path(), json!({"operation":"read_events","after":0}));
        if page["result"]["high_water"] == 2 {
            break page["result"].clone();
        }
        assert!(Instant::now() < deadline, "{page}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(events["events"][1]["turn_id"], *turn);
    assert_eq!(events["events"][1]["kind"], "failed");
    drop(server);
    let _restarted = start();
    assert_eq!(
        request(root.path(), json!({"operation":"read_events","after":0}))["result"],
        events
    );
    assert_eq!(
        request(root.path(), json!({"operation":"read_events","after":2}))["result"]["events"],
        json!([])
    );
}
