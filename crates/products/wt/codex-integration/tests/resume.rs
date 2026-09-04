use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

struct Runtime {
    root: tempfile::TempDir,
}

impl Runtime {
    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .env("HOME", self.root.path())
            .env("CODEX_HOME", self.root.path().join(".codex"))
            .env("TMUX_TMPDIR", self.root.path())
            .env_remove("TMUX");
        command
    }

    fn resume(&self, thread: &str) -> Output {
        self.command(env!("CARGO_BIN_EXE_wt-codex-integration"))
            .args(["runtime-resume", thread])
            .output()
            .unwrap()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.command("/usr/bin/tmux").arg("kill-server").output();
    }
}

#[test]
fn resume_restores_a_missing_pane_reuses_it_and_rejects_unknown_threads() {
    let runtime = Runtime {
        root: tempfile::tempdir().unwrap(),
    };
    let codex = runtime
        .root
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(codex.parent().unwrap()).unwrap();
    // The process boundary supplies persisted App Server state; tmux is real and isolated.
    // No turn/start or thread/start is accepted: resume must not create new work.
    fs::write(
        &codex,
        r#"#!/bin/sh
set -eu
case "$*" in
  'app-server daemon start') exit 0 ;;
  '--remote unix:// resume thread-1') exec sleep 120 ;;
  'app-server proxy') ;;
  *) exit 64 ;;
esac
while IFS= read -r request; do
  id=$(printf '%s' "$request" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$request" in
    *'"method":"initialized"'*) continue ;;
    *'"method":"initialize"'*) result='{}' ;;
    *'"threadId":"missing"'*)
      printf '{"id":%s,"error":{"message":"thread not found"}}\n' "$id"
      continue ;;
    *'"method":"thread/resume"'*|*'"method":"thread/read"'*)
      result='{"thread":{"id":"thread-1","status":{"type":"idle"},"turns":[]}}' ;;
    *) exit 65 ;;
  esac
  printf '{"id":%s,"result":%s}\n' "$id" "$result"
done
"#,
    )
    .unwrap();
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

    let resume = || {
        let output = runtime.resume("thread-1");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let state: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(state["status"], "idle");
        assert!(state["active_turn_id"].is_null());
        state["pane_id"].as_str().unwrap().to_owned()
    };
    let first = resume();
    assert_eq!(resume(), first);
    assert!(runtime
        .command("/usr/bin/tmux")
        .args(["kill-pane", "-t", &first])
        .status()
        .unwrap()
        .success());
    let recovered = resume();
    assert_ne!(recovered, first);
    assert_eq!(resume(), recovered);
    let missing = runtime.resume("missing");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("thread not found"));
    assert_eq!(resume(), recovered);
    let panes = runtime
        .command("/usr/bin/tmux")
        .args(["list-panes", "-a", "-F", "#{pane_id}"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(panes.stdout).unwrap().lines().count(), 2);
}
