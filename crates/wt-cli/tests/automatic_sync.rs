use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use wt_command::cmd;

#[test]
fn new_requires_a_terminal_before_contacting_server() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let helper = bin.join("wt-server");
    fs::write(
        &helper,
        "#!/bin/sh\ntouch \"$HOME/server-contacted\"\nexit 2\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "new")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!temp.path().join("server-contacted").exists());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @"wt: `wt new` requires an interactive terminal\n");
}

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
      *'"git_user_name":"Lucas Ávila"'*'"git_user_email":"lucaxx@gmail.com"'*)
        : > "$state"
        printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"setup","source":"git@example.test:repo.git","guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
        ;;
      *) exit 2 ;;
    esac
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
    fs::create_dir(temp.path().join(".ssh")).unwrap();
    let key = temp.path().join(".ssh/id_ed25519");
    let generated = cmd!("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", &key)
        .output()
        .unwrap();
    assert!(generated.status.success());
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
        "-qfec",
        format!("{} new", env!("CARGO_BIN_EXE_wt")),
        "/dev/null"
    )
    .env("HOME", temp.path())
    .env("PATH", &path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    created
        .stdin
        .take()
        .unwrap()
        .write_all(b"repo-feature\ngit@example.test:repo.git\n\n\n\n\n\n")
        .unwrap();
    let created = created.wait_with_output().unwrap();
    assert!(
        created.status.success(),
        "{}{}",
        String::from_utf8_lossy(&created.stdout),
        String::from_utf8_lossy(&created.stderr)
    );
    let transcript = String::from_utf8_lossy(&created.stdout).replace('\r', "");
    let completed = transcript
        .find("local.repo-feature\tsetup")
        .map(|start| &transcript[start..])
        .expect("creation result is present in the terminal transcript");
    insta::assert_snapshot!(
        completed,
        @r###"
        local.repo-feature	setup	192.0.2.2

        Start setup: ssh local.repo-feature
        Guest host: ssh local.repo-feature-host
        Endpoint: wt@192.0.2.2:22
        "###
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("helper-attempts")).unwrap(),
        "1\n"
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    insta::assert_snapshot!(
        "automatically_synced_ssh_config",
        managed.replace(&temp.path().display().to_string(), "[HOME]")
    );

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
