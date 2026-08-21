use std::fs;
use std::os::unix::fs::PermissionsExt;
use wt_client::cmd;

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn test_home() -> (tempfile::TempDir, std::ffi::OsString) {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    write_executable(
        &bin.join("wt-server"),
        r#"#!/bin/sh
set -eu
request=$(cat)
case "$request" in
  *'"operation":"list"'*)
    printf '%s\n' '{"protocol_version":2,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"jsdev","owner":"tester","status":"running","kind":"devcontainer","source":"git@example.test:group/repo.git","git_base":"main","git_prefix":"jsdev/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}]}}'
    ;;
  *) exit 2 ;;
esac
"#,
    );
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    (temp, path)
}

#[test]
fn ssh_syncs_then_execs_the_qualified_alias() {
    let (temp, path) = test_home();
    write_executable(
        &temp.path().join("bin/ssh"),
        r#"#!/bin/sh
set -eu
test -f "$HOME/.ssh/wt/config"
printf '%s\n' "$@" > "$HOME/ssh-args"
exit 23
"#,
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "ssh", "jsdev")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(23));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(temp.path().join("ssh-args")).unwrap(),
        "--\nars.jsdev\n"
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    assert!(managed.contains("Host ars.jsdev"));
}

#[test]
fn ssh_does_not_start_when_sync_fails() {
    let (temp, path) = test_home();
    fs::create_dir(temp.path().join(".ssh")).unwrap();
    fs::write(
        temp.path().join(".ssh/config"),
        "Host *\n  ServerAliveInterval 60\n",
    )
    .unwrap();
    write_executable(
        &temp.path().join("bin/ssh"),
        "#!/bin/sh\ntouch \"$HOME/ssh-started\"\n",
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "ssh", "ars.jsdev")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!temp.path().join("ssh-started").exists());
    let stderr = String::from_utf8_lossy(&output.stderr)
        .replace(&temp.path().display().to_string(), "[HOME]");
    insta::assert_snapshot!(stderr, @r###"
    wt: configure WT SSH aliases in [HOME]/.ssh/config
    add `Include ~/.ssh/wt/config` outside any `Host` or `Match` block, then run `wt sync`: [HOME]/.ssh/config does not load WT SSH aliases in its global configuration
    "###);
}
