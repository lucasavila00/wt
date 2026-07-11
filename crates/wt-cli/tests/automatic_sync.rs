use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn new_and_rm_always_sync_ssh_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let helper = bin.join("wt-local");
    fs::write(
        &helper,
        r#"#!/bin/sh
set -eu
request=$(cat)
state="$HOME/helper-state"
case "$request" in
  *'"operation":"create"'*)
    : > "$state"
    printf '%s\n' '{"protocol_version":2,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"running","source":"git@example.test:repo.git","guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
    ;;
  *'"operation":"delete"'*)
    rm -f "$state"
    printf '%s\n' '{"protocol_version":2,"outcome":"ok","response":{"response":"deleted","name":"repo-feature"}}'
    ;;
  *'"operation":"list"'*)
    if [ -f "$state" ]; then
      printf '%s\n' '{"protocol_version":2,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"running","source":"git@example.test:repo.git","guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}]}}'
    else
      printf '%s\n' '{"protocol_version":2,"outcome":"ok","response":{"response":"instances","instances":[]}}'
    fi
    ;;
  *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let identity = temp.path().join("identity");
    fs::write(&identity, "test").unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let created = Command::new(env!("CARGO_BIN_EXE_wt"))
        .args([
            "new",
            "git@example.test:repo.git",
            "repo-feature",
            "--identity",
        ])
        .arg(&identity)
        .env("HOME", temp.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    assert!(managed.contains("Host repo-feature"));

    let removed = Command::new(env!("CARGO_BIN_EXE_wt"))
        .args(["rm", "repo-feature"])
        .env("HOME", temp.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    assert!(!managed.contains("Host repo-feature"));
}
