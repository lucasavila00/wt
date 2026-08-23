use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

fn run_codex(codex: &Path, home: &Path, ignore_checks: bool) {
    let sync_marker = home.join("sync-called");
    let _ = fs::remove_file(&sync_marker);

    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "umask 0077; exec \"$@\"", "sh"])
        .arg(codex)
        .args(["resume", "thread id", "--all"])
        .env("HOME", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("WT_CODEX_TEST_ENV", "unchanged")
        .env(
            "IGNORE_CODEX_WT_CHECKS",
            if ignore_checks { "true" } else { "false" },
        )
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"unchanged\n")
        .unwrap();
    let run = child.wait_with_output().unwrap();

    assert_eq!(run.status.code(), Some(23));
    let expected_stdout = format!(
        "{}argc=3\narg=[resume]\narg=[thread id]\narg=[--all]\nenv=unchanged\ncwd={}\numask=0077\nstdin=unchanged\npid={}\n",
        if ignore_checks {
            ""
        } else {
            "Syncing shared Codex history before starting Codex. Set IGNORE_CODEX_WT_CHECKS=true to skip this synchronization.\n"
        },
        home.display(),
        pid
    );
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected_stdout);
    assert_eq!(sync_marker.exists(), !ignore_checks);
    assert_eq!(String::from_utf8(run.stderr).unwrap(), "stderr=unchanged\n");
}

#[test]
fn both_image_entrypoints_synchronize_before_execing_the_fixed_real_codex() {
    let temp = tempfile::tempdir().unwrap();
    let real_codex = temp
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(real_codex.parent().unwrap()).unwrap();
    fs::write(
        &real_codex,
        concat!(
            "#!/bin/sh\n",
            "case \"${1-}\" in\n",
            "  --version) printf 'codex-cli 0.149.0\\n' ;;\n",
            "  app-server)\n",
            "    printf 'synced\\n' > \"$HOME/sync-called\"\n",
            "    while IFS= read -r line; do\n",
            "      case \"$line\" in\n",
            "        *'\"method\":\"initialize\"'*)\n",
            "          printf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"codexHome\":\"%s\"}}\\n' \"$CODEX_HOME\" ;;\n",
            "        *'\"method\":\"thread/list\"'*)\n",
            "          id=$(printf '%s' \"$line\" | sed -n 's/.*\"id\":\\([0-9][0-9]*\\).*/\\1/p')\n",
            "          printf '{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"data\":[],\"nextCursor\":null}}\\n' \"$id\" ;;\n",
            "      esac\n",
            "    done ;;\n",
            "  *)\n",
            "    printf 'argc=%s\\n' \"$#\"\n",
            "    for arg do printf 'arg=[%s]\\n' \"$arg\"; done\n",
            "    printf 'env=%s\\n' \"$WT_CODEX_TEST_ENV\"\n",
            "    printf 'cwd=%s\\n' \"$PWD\"\n",
            "    printf 'umask=%s\\n' \"$(umask)\"\n",
            "    IFS= read -r input\n",
            "    printf 'stdin=%s\\n' \"$input\"\n",
            "    printf 'pid=%s\\n' \"$$\"\n",
            "    printf 'stderr=unchanged\\n' >&2\n",
            "    exit 23 ;;\n",
            "esac\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();

    let user_bin = temp.path().join(".local/bin");
    let system_bin = temp.path().join("system-bin");
    fs::create_dir_all(&user_bin).unwrap();
    fs::create_dir(&system_bin).unwrap();
    let integration = Path::new(env!("CARGO_BIN_EXE_wt-codex-integration"));
    symlink(integration, user_bin.join("codex")).unwrap();
    symlink(integration, system_bin.join("codex")).unwrap();

    run_codex(&user_bin.join("codex"), temp.path(), false);
    run_codex(&system_bin.join("codex"), temp.path(), false);
    run_codex(&system_bin.join("codex"), temp.path(), true);
}

#[test]
fn version_does_not_require_history_synchronization() {
    let temp = tempfile::tempdir().unwrap();
    let real_codex = temp
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(real_codex.parent().unwrap()).unwrap();
    fs::write(
        &real_codex,
        r#"#!/bin/sh
case "${1-}" in
  --version) printf 'codex-cli 0.149.0\n' ;;
  app-server)
    while IFS= read -r line; do
      case "$line" in
        *'"method":"initialize"'*)
          printf '{"jsonrpc":"2.0","id":1,"result":{"codexHome":"%s"}}\n' "$CODEX_HOME"
          ;;
        *'"method":"thread/list"'*)
          id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
          printf '{"jsonrpc":"2.0","id":%s,"result":{"data":[],"nextCursor":null}}\n' "$id"
          ;;
      esac
    done
    ;;
  *) exit 64 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();
    let codex = temp.path().join("codex");
    symlink(env!("CARGO_BIN_EXE_wt-codex-integration"), &codex).unwrap();

    let output = Command::new(codex)
        .arg("--version")
        .env("HOME", temp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"codex-cli 0.149.0\n");
}

#[test]
fn stop_hook_continues_when_the_relay_rejects_the_report() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("gateway.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let relay = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        stream
            .write_all(b"{\"ok\":false,\"error\":\"forced relay failure\"}\n")
            .unwrap();
    });
    let payload = b"{\"session_id\":\"123e4567-e89b-12d3-a456-426614174000\",\"cwd\":\"/home/wt/project\",\"hook_event_name\":\"Stop\"}\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_wt-codex-integration"))
        .arg("report-hook")
        .env("HOME", temp.path())
        .env("WT_BYOBU_PANE", "%1")
        .env("WT_BYOBU_SESSION", "wt-test")
        .env("WT_AGENT_TOOL_TEST_SOCKET", socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(payload).unwrap();
    let output = child.wait_with_output().unwrap();
    relay.join().unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"{\"continue\":true}\n");
    assert!(output.stderr.is_empty());
}
