use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};

fn run_codex(codex: &Path, home: &Path, reconciliation_fails: bool, ignore_checks: bool) {
    let marker = home.join(".codex/reconciled");
    let _ = fs::remove_file(&marker);
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
        .env(
            "WT_CODEX_TEST_RECONCILE_FAIL",
            if reconciliation_fails { "1" } else { "0" },
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

    let blocked = reconciliation_fails && !ignore_checks;
    assert_eq!(run.status.code(), Some(if blocked { 1 } else { 23 }));
    let expected_stdout = if blocked {
        String::new()
    } else {
        format!(
            "reconciled={}\nargc=3\narg=[resume]\narg=[thread id]\narg=[--all]\nenv=unchanged\ncwd={}\numask=0077\nstdin=unchanged\npid={}\n",
            if ignore_checks { "not-refreshed" } else { "before-exec" },
            home.display(),
            pid
        )
    };
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected_stdout);
    let expected_stderr = if blocked {
        format!(
            "wt-codex-integration: Codex reconciliation failed: Codex app-server stopped before initialize replied: refresh unavailable; full diagnostic recorded at {}/.local/state/wt/codex-reconciliation.log; set IGNORE_CODEX_WT_CHECKS=true to start Codex without reconciliation\n",
            home.display()
        )
    } else {
        "stderr=unchanged\n".to_owned()
    };
    assert_eq!(String::from_utf8(run.stderr).unwrap(), expected_stderr);
    let log = home.join(".local/state/wt/codex-reconciliation.log");
    if blocked {
        let diagnostic = fs::read_to_string(&log).unwrap();
        assert!(diagnostic.starts_with("timestamp_unix="));
        assert!(diagnostic.contains(" pid="));
        assert!(diagnostic.ends_with(
            "\nCodex app-server stopped before initialize replied: refresh unavailable\n\n"
        ));
        assert_eq!(
            fs::metadata(home.join(".local/state/wt")).unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(fs::metadata(log).unwrap().mode() & 0o777, 0o600);
    } else if !reconciliation_fails {
        assert!(!log.exists());
    }
}

#[test]
fn both_image_entrypoints_reconcile_then_exec_the_fixed_real_codex() {
    let temp = tempfile::tempdir().unwrap();
    let real_codex = temp
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(real_codex.parent().unwrap()).unwrap();
    fs::write(
        &real_codex,
        concat!(
            "#!/bin/sh\n",
            "if test \"${1-}\" = app-server; then\n",
            "  if test \"$WT_CODEX_TEST_RECONCILE_FAIL\" = 1; then\n",
            "    printf 'refresh unavailable\\n' >&2\n",
            "    exit 7\n",
            "  fi\n",
            "  indexed=false\n",
            "  while IFS= read -r line; do\n",
            "    case \"$line\" in\n",
            "      *'\"method\":\"initialize\"'*)\n",
            "        printf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"codexHome\":\"%s\"}}\\n' \"$CODEX_HOME\"\n",
            "        ;;\n",
            "      *'\"method\":\"thread/list\"'*)\n",
            "        id=$(printf '%s' \"$line\" | sed -n 's/.*\"id\":\\([0-9][0-9]*\\).*/\\1/p')\n",
            "        if \"$indexed\"; then sleep 0.1; : > \"$CODEX_HOME/reconciled\"; data='[{\"id\":\"33333333-3333-4333-8333-333333333333\"}]'; else data='[]'; fi\n",
            "        printf '{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"data\":%s,\"nextCursor\":null}}\\n' \"$id\" \"$data\"\n",
            "        ;;\n",
            "      *'\"method\":\"thread/read\"'*)\n",
            "        id=$(printf '%s' \"$line\" | sed -n 's/.*\"id\":\\([0-9][0-9]*\\).*/\\1/p')\n",
            "        indexed=true\n",
            "        printf '{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"thread\":{}}}\\n' \"$id\"\n",
            "        ;;\n",
            "    esac\n",
            "  done\n",
            "  exit 0\n",
            "fi\n",
            "if test -f \"$CODEX_HOME/reconciled\"; then\n",
            "  printf 'reconciled=before-exec\\n'\n",
            "else\n",
            "  printf 'reconciled=not-refreshed\\n'\n",
            "fi\n",
            "printf 'argc=%s\\n' \"$#\"\n",
            "for arg do printf 'arg=[%s]\\n' \"$arg\"; done\n",
            "printf 'env=%s\\n' \"$WT_CODEX_TEST_ENV\"\n",
            "printf 'cwd=%s\\n' \"$PWD\"\n",
            "printf 'umask=%s\\n' \"$(umask)\"\n",
            "IFS= read -r input\n",
            "printf 'stdin=%s\\n' \"$input\"\n",
            "printf 'pid=%s\\n' \"$$\"\n",
            "printf 'stderr=unchanged\\n' >&2\n",
            "exit 23\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();
    let sessions = temp.path().join(".codex/sessions/2026/08/20");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("rollout-2026-08-20T10-00-00-33333333-3333-4333-8333-333333333333.jsonl"),
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"33333333-3333-4333-8333-333333333333\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}\n"
        ),
    )
    .unwrap();

    let user_bin = temp.path().join(".local/bin");
    let system_bin = temp.path().join("system-bin");
    fs::create_dir_all(&user_bin).unwrap();
    fs::create_dir(&system_bin).unwrap();
    let integration = Path::new(env!("CARGO_BIN_EXE_wt-codex-integration"));
    symlink(integration, user_bin.join("codex")).unwrap();
    symlink(integration, system_bin.join("codex")).unwrap();

    run_codex(&user_bin.join("codex"), temp.path(), false, false);
    run_codex(&system_bin.join("codex"), temp.path(), true, false);
    run_codex(&system_bin.join("codex"), temp.path(), true, true);
}
