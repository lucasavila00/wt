use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};

const GENERATION: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn run_codex(codex: &Path, home: &Path, ready: bool, ignore_checks: bool) {
    let state = home.join(".local/state/wt");
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("codex-reconciliation-desired"),
        format!("{GENERATION}\n"),
    )
    .unwrap();
    if ready {
        fs::write(
            state.join("codex-reconciliation-status.json"),
            format!(
                "{{\"state\":\"ready\",\"generation\":\"{GENERATION}\",\"codex_version\":\"codex-cli 0.149.0\"}}\n"
            ),
        )
        .unwrap();
    } else {
        let _ = fs::remove_file(state.join("codex-reconciliation-status.json"));
    }

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

    let blocked = !ready && !ignore_checks;
    assert_eq!(run.status.code(), Some(if blocked { 1 } else { 23 }));
    let expected_stdout = if blocked {
        String::new()
    } else {
        format!(
            "argc=3\narg=[resume]\narg=[thread id]\narg=[--all]\nenv=unchanged\ncwd={}\numask=0077\nstdin=unchanged\npid={}\n",
            home.display(),
            pid
        )
    };
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected_stdout);
    let expected_stderr = if blocked {
        "wt-codex-integration: Codex history preparation is pending; retry shortly or set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history\n"
            .to_owned()
    } else {
        "stderr=unchanged\n".to_owned()
    };
    assert_eq!(String::from_utf8(run.stderr).unwrap(), expected_stderr);
}

#[test]
fn both_image_entrypoints_check_readiness_then_exec_the_fixed_real_codex() {
    let temp = tempfile::tempdir().unwrap();
    let real_codex = temp
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(real_codex.parent().unwrap()).unwrap();
    fs::write(
        &real_codex,
        concat!(
            "#!/bin/sh\n",
            "if test \"${1-}\" = --version; then printf 'codex-cli 0.149.0\\n'; exit 0; fi\n",
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

    let user_bin = temp.path().join(".local/bin");
    let system_bin = temp.path().join("system-bin");
    fs::create_dir_all(&user_bin).unwrap();
    fs::create_dir(&system_bin).unwrap();
    let integration = Path::new(env!("CARGO_BIN_EXE_wt-codex-integration"));
    symlink(integration, user_bin.join("codex")).unwrap();
    symlink(integration, system_bin.join("codex")).unwrap();

    run_codex(&user_bin.join("codex"), temp.path(), true, false);
    run_codex(&system_bin.join("codex"), temp.path(), false, false);
    run_codex(&system_bin.join("codex"), temp.path(), false, true);
}

#[test]
fn version_does_not_require_background_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let real_codex = temp
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(real_codex.parent().unwrap()).unwrap();
    fs::write(&real_codex, "#!/bin/sh\nprintf 'codex-cli 0.149.0\\n'\n").unwrap();
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
fn background_worker_publishes_the_applied_generation_and_version() {
    let temp = tempfile::tempdir().unwrap();
    let real_codex = temp
        .path()
        .join(".codex/packages/standalone/current/bin/codex");
    fs::create_dir_all(real_codex.parent().unwrap()).unwrap();
    fs::write(&real_codex, "#!/bin/sh\nprintf 'codex-cli 0.149.0\\n'\n").unwrap();
    fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();
    let state = temp.path().join(".local/state/wt");
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("codex-reconciliation-desired"),
        format!("{GENERATION}\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wt-codex-integration"))
        .arg("reconcile-worker")
        .env("HOME", temp.path())
        .env("CODEX_HOME", temp.path().join(".codex"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(state.join("codex-reconciliation-status.json")).unwrap(),
        format!(
            "{{\"state\":\"ready\",\"generation\":\"{GENERATION}\",\"codex_version\":\"codex-cli 0.149.0\"}}\n"
        )
    );
    assert_eq!(fs::metadata(&state).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(state.join("codex-reconciliation-status.json"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
}
