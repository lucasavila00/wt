use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use wt_command::cmd;

#[test]
fn new_and_rm_always_sync_ssh_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let helper = bin.join("wt-server");
    fs::write(
        &helper,
        r#"#!/bin/sh
set -eu
request=$(cat)
state="$HOME/helper-state"
case "$request" in
  *'"operation":"create"'*)
    attempts="$HOME/helper-attempts"
    count=0
    test ! -f "$attempts" || count=$(cat "$attempts")
    count=$((count + 1))
    printf '%s\n' "$count" > "$attempts"
    case "$request" in
      *'"git_passphrase":"secret"'*'"git_user_name":"Lucas Ávila"'*'"git_user_email":"lucaxx@gmail.com"'*)
        : > "$state"
        printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"provisioning","source":"git@example.test:repo.git"}}}'
        ;;
      *)
        printf '%s\n' '{"protocol_version":1,"outcome":"error","error":{"code":"invalid_git_passphrase","message":"Git identity: invalid private key passphrase"}}'
        ;;
    esac
    ;;
  *'"operation":"logs"'*'"offset":0'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"logs","chunk":"YnVpbGRpbmcK","next_offset":9,"status":"running"}}'
    ;;
  *'"operation":"logs"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"logs","chunk":"","next_offset":9,"status":"running"}}'
    ;;
  *'"operation":"get"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"running","source":"git@example.test:repo.git","guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}}}'
    ;;
  *'"operation":"delete"'*)
    rm -f "$state"
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"deleted","name":"repo-feature"}}'
    ;;
  *'"operation":"list"'*)
    if [ -f "$state" ]; then
      printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"running","source":"git@example.test:repo.git","guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}]}}'
    else
      printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instances","instances":[]}}'
    fi
    ;;
  *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".gitconfig"),
        "[user]\n\tname = Lucas Ávila\n\temail = lucaxx@gmail.com\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let mut created = cmd!(
        "script",
        "-q",
        "-e",
        "-c",
        &format!(
            "{} new git@example.test:repo.git repo-feature",
            env!("CARGO_BIN_EXE_wt")
        ),
        "/dev/null",
    )
    .env("HOME", temp.path())
    .env("PATH", &path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let mut stdout = created.stdout.take().unwrap();
    let mut transcript = Vec::new();
    let prompt = b"Server Git SSH key passphrase: ";
    for passphrase in [b"wrong-one\n".as_slice(), b"wrong-two\n", b"secret\n"] {
        loop {
            let mut byte = [0];
            assert_eq!(stdout.read(&mut byte).unwrap(), 1);
            transcript.push(byte[0]);
            if transcript.ends_with(prompt) {
                break;
            }
        }
        created
            .stdin
            .as_mut()
            .unwrap()
            .write_all(passphrase)
            .unwrap();
    }
    drop(created.stdin.take());
    let mut remaining = Vec::new();
    stdout.read_to_end(&mut remaining).unwrap();
    transcript.extend(remaining);
    let output = created.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&transcript),
        String::from_utf8_lossy(&output.stderr)
    );
    let transcript = String::from_utf8_lossy(&transcript);
    assert!(!transcript.contains("wrong-one"));
    assert!(!transcript.contains("wrong-two"));
    assert!(!transcript.contains("secret"));
    let normalized_transcript = transcript
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(
        normalized_transcript,
        @r###"
        To clone git@example.test:repo.git into local.repo-feature, WT must unlock the Git SSH key configured on that context's server. This may differ from the SSH key your client uses to connect to the server.
        Server Git SSH key passphrase:
        Git identity: invalid private key passphrase; 2 attempts remaining.
        Server Git SSH key passphrase:
        Git identity: invalid private key passphrase; 1 attempt remaining.
        Server Git SSH key passphrase:
        building
        local.repo-feature	running	192.0.2.2

        App shell: ssh local.repo-feature
        Editor / raw app SSH: ssh local.repo-feature-vs
        Guest host: ssh local.repo-feature-host
        Endpoint: wt@192.0.2.2:22
        "###
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("helper-attempts")).unwrap(),
        "3\n"
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    insta::assert_snapshot!(
        "automatically_synced_ssh_config",
        managed.replace(&temp.path().display().to_string(), "[HOME]")
    );

    let logs = cmd!(env!("CARGO_BIN_EXE_wt"), "logs", "repo-feature")
        .env("HOME", temp.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        logs.status.success(),
        "{}",
        String::from_utf8_lossy(&logs.stderr)
    );
    insta::assert_snapshot!(String::from_utf8_lossy(&logs.stdout), @"building");

    let removed = cmd!(env!("CARGO_BIN_EXE_wt"), "rm", "repo-feature")
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
