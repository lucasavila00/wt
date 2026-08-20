use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

#[test]
fn install_runs_and_remove_restores_the_real_codex() {
    let temp = tempfile::tempdir().unwrap();
    let codex = temp.path().join("codex");
    fs::write(
        &codex,
        concat!(
            "#!/bin/sh\n",
            "printf 'argc=%s\\n' \"$#\"\n",
            "for arg do printf 'arg=[%s]\\n' \"$arg\"; done\n",
            "printf 'env=%s\\n' \"$WT_CODEX_TEST_ENV\"\n",
            "printf 'cwd=%s\\n' \"$PWD\"\n",
            "IFS= read -r input\n",
            "printf 'stdin=%s\\n' \"$input\"\n",
            "printf 'pid=%s\\n' \"$$\"\n",
            "printf 'stderr=unchanged\\n' >&2\n",
            "exit 23\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&codex, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths([temp.path()]).unwrap();
    let wt_codex = env!("CARGO_BIN_EXE_wt-codex");

    let install = Command::new(wt_codex)
        .arg("install")
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let mut run_command = Command::new(&codex);
    run_command
        .args(["resume", "thread id", "--all"])
        .env("PATH", &path)
        .env("HOME", temp.path())
        .env("CODEX_HOME", temp.path().join("codex-home"))
        .env("WT_CODEX_TEST_ENV", "unchanged")
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = run_command.spawn().unwrap();
    let pid = child.id();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"unchanged\n")
        .unwrap();
    let run = child.wait_with_output().unwrap();
    assert_eq!(run.status.code(), Some(23));
    assert_eq!(
        String::from_utf8(run.stdout).unwrap(),
        format!(
            "argc=3\narg=[resume]\narg=[thread id]\narg=[--all]\nenv=unchanged\ncwd={}\nstdin=unchanged\npid={}\n",
            temp.path().display(),
            pid
        )
    );
    assert_eq!(run.stderr, b"stderr=unchanged\n");

    let remove = Command::new(wt_codex)
        .arg("remove")
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        remove.status.success(),
        "{}",
        String::from_utf8_lossy(&remove.stderr)
    );
    assert!(fs::symlink_metadata(&codex).unwrap().file_type().is_file());
    assert!(!temp.path().join(".codex.wt-real").exists());
}
